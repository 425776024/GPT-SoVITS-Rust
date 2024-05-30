use std::path::Path;
use std::time::{Duration, Instant};
use ndarray::{Array, Array1, Array2, Array3, Array4, ArrayView1, ArrayView2, ArrayViewD, Axis, IxDyn, s};
use ort::{CPUExecutionProvider, CUDAExecutionProvider, ExecutionProvider, ExecutionProviderDispatch, GraphOptimizationLevel, inputs, Session, SessionBuilder, Tensor};
use rsmpeg::ffi::cos;
// use sdl2::audio::AudioSpecDesired;
use tokenizers::Tokenizer;
use crate::ffmpeg_util::ffmpeg_utils::FfmpegUtils;
use crate::ffmpeg_utils::FfmpegUtils;
use crate::infer_commands::INFER_WAV_SESSION;
use crate::text_utils::{CHINESE_LANG, TextUtils};
use crate::tts_sovits::text_utils::{CHINESE_LANG, TextUtils};

pub struct ChBertUtils {
    pub tokenizer: Tokenizer,
}


pub fn hanning(M: i64) -> Array1<f32> {
    let pi: f32 = std::f32::consts::PI;
    let res: Array1<f32> = {
        if M < 1 {
            Array1::from_vec(vec![])
        } else if M == 1 {
            Array1::ones(1)
        } else {
            let n: Vec<f32> = (1 - M..M).step_by(2).map(|x| {
                let v1 = pi * (x as f32) / (M - 1) as f32;
                let v2 = unsafe {
                    0.5 + 0.5 * cos(v1 as f64)
                };
                v2 as f32
            }).collect();
            Array1::from_vec(n)
        }
    };
    res
}

impl ChBertUtils {
    pub fn init(tokenizer_json_path: &str) -> Self {
        let tokenizer = Tokenizer::from_file(tokenizer_json_path).unwrap();
        ChBertUtils { tokenizer }
    }

    pub fn load_model(bert_model_path: &str) -> ort::Result<Session> {
        let model_bytes = std::fs::read(bert_model_path).unwrap();

        let cuda_build = CUDAExecutionProvider::default().build();
        let cuda_is_available = cuda_build.is_available().unwrap();
        println!("load on gpu:{}", cuda_is_available);

        let execution_providers = vec![cuda_build];
        let mut session_nrf_builder = SessionBuilder::new();

        let mut session_nrf = session_nrf_builder.unwrap()
            .with_optimization_level(GraphOptimizationLevel::Level3).unwrap()
            .with_execution_providers(execution_providers).unwrap()
            .commit_from_memory(&model_bytes);
        if session_nrf.is_err() {
            let execution_providers = vec![CPUExecutionProvider::default().build()];
            session_nrf = SessionBuilder::new().unwrap()
                .with_optimization_level(GraphOptimizationLevel::Level3).unwrap()
                .with_execution_providers(execution_providers).unwrap()
                .commit_from_memory(&model_bytes);
        }

        return session_nrf;
    }


    // 返回最终的混合中英文句子features
    pub fn get_bert_features(tokenizer: &Tokenizer,
                             bert_model: &Session,
                             phones_list: &mut Vec<Vec<usize>>,
                             word2ph_list: &Vec<Vec<usize>>,
                             norm_text_list: &Vec<String>,
                             language_list: &Vec<String>)
                             -> (Array2<f32>, Vec<usize>, String) {
        let mut bert_features = vec![];
        let mut phones_list_unpack = vec![];
        let mut norm_text_str = "".to_string();
        for i in 0..language_list.len() {
            let phones_len = phones_list[i].len();
            let word2ph = &word2ph_list[i];

            norm_text_str = norm_text_str + &norm_text_list[i];
            phones_list_unpack.append(&mut phones_list[i]);

            if language_list[i] == CHINESE_LANG {
                let encoding_opt = tokenizer.encode(norm_text_list[i].as_str(), true);
                if encoding_opt.is_err() {
                    continue;
                }
                let encoding = encoding_opt.unwrap();

                let input_ids = encoding.get_ids();
                let attention_mask = encoding.get_attention_mask();
                let token_type_ids = encoding.get_type_ids();

                let input_ids: Array2<i64> = ndarray::Array1::from_vec(input_ids.to_vec()).insert_axis(Axis(0)).mapv(|x| x as i64);
                let attention_mask: Array2<i64> = ndarray::Array1::from_vec(attention_mask.to_vec()).insert_axis(Axis(0)).mapv(|x| x as i64);
                let token_type_ids: Array2<i64> = ndarray::Array1::from_vec(token_type_ids.to_vec()).insert_axis(Axis(0)).mapv(|x| x as i64);

                let input_tensor_value = inputs![input_ids, attention_mask, token_type_ids].unwrap();
                let generator_source = bert_model.run(input_tensor_value);
                let generator_source = generator_source.unwrap();

                let hidden_states = generator_source["hidden_states"].try_extract_tensor().unwrap();
                // [1, 32, 1024] -> [0,1:-1,:]
                let hidden_states: Array2<f32> = hidden_states.view().slice(s![0,1..-1,..;1]).to_owned();

                let mut phone_level_feature = vec![];
                for (i, &w2) in word2ph.iter().enumerate() {
                    let mut repeat_features = vec![];
                    for _ in 0..w2 {
                        let repeat_feature_i: Array1<f32> = hidden_states.slice(s![i,..;1]).to_owned();
                        repeat_features.push(repeat_feature_i);
                    }

                    let repeat_features_view: Vec<ArrayView1<f32>> = repeat_features.iter().map(|v| v.view()).collect();

                    let repeat_feature: Array2<f32> = ndarray::stack(Axis(0), &repeat_features_view).unwrap();
                    phone_level_feature.push(repeat_feature);
                }

                let phone_level_feature: Vec<ArrayView2<f32>> = phone_level_feature.iter().map(|v| v.view()).collect();

                let phone_level_feature = ndarray::concatenate(Axis(0), &phone_level_feature).unwrap();
                let phone_level_feature_t: Array2<f32> = ndarray::ArrayBase::t(&phone_level_feature).to_owned();
                bert_features.push(phone_level_feature_t);
            } else {
                let bert: Array2<f32> = Array2::zeros((1024, phones_len));
                bert_features.push(bert);
            }
        }
        let bert_features_view: Vec<ArrayView2<f32>> = bert_features.iter().map(|v| v.view()).collect();
        let bert_features: Array2<f32> = ndarray::concatenate(Axis(1), &bert_features_view).unwrap();

        (bert_features, phones_list_unpack, norm_text_str)
    }
}

/*
生成音频
**/
pub fn wav_maker(
    ssl_model: &Session,
    vq_model_latent: &Session,
    t2s_first_stage_decoder: &Session,
    t2s_stage_decoder: &Session,
    vq_model: &Session,
    wav16k_arr: &Array2<f32>,
    wav32k_arr: &Array2<f32>,
    bert_features1: &Array2<f32>,
    bert_features2: &Array2<f32>,
    phones_list_unpack1: &Vec<usize>,
    phones_list_unpack2: &Vec<usize>,
    top_k: i64,
    temperature: f32,
) -> Result<Vec<i16>, String> {
    let input_wav16k = inputs![wav16k_arr.clone()].unwrap();
    let ssl_content = ssl_model.run(input_wav16k);
    let ssl_content = ssl_content.unwrap();
    let ssl_content = ssl_content["output"].try_extract_tensor::<f32>().unwrap();

    let hop_length = 640;
    let win_length = 2048;
    let hann_window = hanning(win_length);

    // [1, 768, 239]
    let ssl_content: Array3<f32> = ssl_content.view().slice(s![..,..,..]).to_owned();

    let input_ssl_content = inputs![ssl_content].unwrap();
    let codes = vq_model_latent.run(input_ssl_content);
    let codes = codes.unwrap();
    let codes = codes["output"].try_extract_tensor::<i64>().unwrap();
    let prompt: Array2<i64> = codes.view().slice(s![0,..,..]).to_owned();

    let top_k: Array1<i64> = ndarray::Array1::from(vec![top_k]);
    let temperature: Array1<f32> = ndarray::Array1::from(vec![temperature]);
    //  合并参考的声音
    let bert: Array3<f32> = ndarray::concatenate(Axis(1), &[bert_features1.view(), bert_features2.view()]).unwrap().insert_axis(Axis(0));

    // 会清空
    let mut _phones_list_unpack1 = phones_list_unpack1.clone();
    let mut _phones_list_unpack2 = phones_list_unpack2.clone();
    _phones_list_unpack1.append(&mut _phones_list_unpack2);

    let all_phoneme_ids: Array2<i64> = Array1::from_vec(_phones_list_unpack1).insert_axis(Axis(0)).mapv(|x| x as i64);
    let text: Array2<i64> = Array1::from_vec(phones_list_unpack2.clone()).insert_axis(Axis(0)).mapv(|x| x as i64);

    let x_example: Array2<f32> = Array2::zeros((all_phoneme_ids.shape()[0], all_phoneme_ids.shape()[1]));

    let start_loop = Instant::now();
    // let first_stage_decoder_input = inputs![all_phoneme_ids,bert,prompt,&top_k,&temperature].unwrap();
    let first_stage_decoder_input = inputs![
        "all_phoneme_ids" => all_phoneme_ids.view(),
        "bert" => bert.view(),
        "prompt" => prompt.view(),
        "top_k" => top_k.view(),
        "temperature" => temperature.view(),
    ].unwrap();
    let t2s_first_stage_out = t2s_first_stage_decoder.run(first_stage_decoder_input);
    let t2s_first_stage_out = t2s_first_stage_out.unwrap();

    let mut y: Array2<i64> = t2s_first_stage_out["y"].try_extract_tensor::<i64>().unwrap().view().slice(s![..,..]).into_owned();
    let mut k: Array4<f32> = t2s_first_stage_out["k"].try_extract_tensor::<f32>().unwrap().view().slice(s![..,..,..,..]).into_owned();
    let mut v: Array4<f32> = t2s_first_stage_out["v"].try_extract_tensor::<f32>().unwrap().view().slice(s![..,..,..,..]).into_owned();
    let mut y_emb: Array3<f32> = t2s_first_stage_out["y_emb"].try_extract_tensor::<f32>().unwrap().view().slice(s![..,..,..]).into_owned();

    let mut y_example: Array2<f32> = Array2::zeros((1, y_emb.shape()[1]));
    let y_example_0: Array2<f32> = Array2::zeros((1, 1));


    let mut loop_idx = 0;
    for idx in 1..1500 {
        y_example = ndarray::concatenate(Axis(1), &[y_example.view(), y_example_0.view()]).unwrap();
        let xy_attn_mask: Array4<f32> = ndarray::concatenate(Axis(1), &[x_example.view(), y_example.view()])
            .unwrap().insert_axis(Axis(0)).insert_axis(Axis(0));


        let t2s_stage_decoder_input = inputs![
        "y" => y.view(),
        "k" => k.view(),
        "v" => v.view(),
        "y_emb" => y_emb.view(),
        "xy_attn_mask" => xy_attn_mask.view(),
        "top_k" => top_k.view(),
        "temperature" => temperature.view(),
        ].unwrap();

        let t2s_stage_decoder_out = t2s_stage_decoder.run(t2s_stage_decoder_input);
        let t2s_stage_decoder_out = t2s_stage_decoder_out.unwrap();

        k = t2s_stage_decoder_out["o_k"].try_extract_tensor::<f32>().unwrap().view().slice(s![..,..,..,..]).into_owned();
        v = t2s_stage_decoder_out["o_v"].try_extract_tensor::<f32>().unwrap().view().slice(s![..,..,..,..]).into_owned();
        y_emb = t2s_stage_decoder_out["o_y_emb"].try_extract_tensor::<f32>().unwrap().view().slice(s![..,..,..]).into_owned();
        let logits: Array1<i64> = t2s_stage_decoder_out["logits"].try_extract_tensor::<i64>().unwrap().view().slice(s![..]).into_owned();
        let samples: Array2<i64> = t2s_stage_decoder_out["samples"].try_extract_tensor::<i64>().unwrap().view().slice(s![..,..]).into_owned();

        y = ndarray::concatenate(Axis(1), &[y.view(), samples.view()]).unwrap();

        let sample = samples.get((0, 0)).unwrap();
        let logit = logits.get(0).unwrap();

        if *logit == 1024 || *sample == 1024 {
            loop_idx = idx;
            break;
        }
    }

    *y.get_mut((0, y.shape()[1] - 1)).unwrap() = 0;

    let pred_semantic: Array3<i64> = y.slice(s![..,y.shape()[1]-loop_idx..]).into_owned().insert_axis(Axis(0));

    let y_len = (pred_semantic.shape()[2] * 2) as i64;
    let y_lengths: Array1<i64> = ndarray::Array1::from(vec![y_len]);
    let text_lengths: Array1<i64> = ndarray::Array1::from(vec![text.shape()[0] as i64]);
    let T = (wav32k_arr.shape()[1] - hop_length) / hop_length + 1;
    let refer_mask: Array3<i64> = Array3::ones((pred_semantic.shape()[0], pred_semantic.shape()[1], T));

    let vq_model_input = inputs![
        "pred_semantic"=>pred_semantic.view(),
        "text"=>text.view(),
        "org_audio"=>wav32k_arr.view(),
        "hann_window"=>hann_window.view(),
        "refer_mask"=>refer_mask.view(),
        "y_lengths"=>y_lengths.view(),
        "text_lengths"=>text_lengths.view(),
    ].unwrap();

    let vq_model_out = vq_model.run(vq_model_input);
    if vq_model_out.is_err() {
        return Err(String::from("Infer error"));
    }

    let vq_model_out = vq_model_out.unwrap();

    let mut audio: Array1<f32> = vq_model_out["audio"].try_extract_tensor::<f32>().unwrap().view().slice(s![0,0,..]).into_owned();

    let mut audio: Vec<f32> = audio.to_vec();
    let max_audio = {
        let mut max_v = 0.0;
        for &v in &audio {
            let v = num::abs(v);
            if v > max_v {
                max_v = v;
            }
        }
        max_v
    };

    let audio_norm = {
        if max_audio > 1.0 {
            let v: Vec<i16> = audio.iter().map(|&x| {
                ((x / max_audio) * 32768.0) as i16
            }).collect();
            v
        } else {
            let v: Vec<i16> = audio.iter().map(|&x| {
                (x * 32768.0) as i16
            }).collect();
            v
        }
    };

    Ok(audio_norm)
}


fn test_infer_wav(
    ssl_model: &Session,
    vq_model_latent: &Session,
    t2s_first_stage_decoder: &Session,
    t2s_stage_decoder: &Session,
    vq_model: &Session,
    wav16k_arr: &Array2<f32>,
    wav32k_arr: &Array2<f32>,
    bert_features1: &Array2<f32>,
    bert_features2: &Array2<f32>,
    phones_list_unpack1: &Vec<usize>,
    phones_list_unpack2: &Vec<usize>,
)
{
    let input_wav16k = inputs![wav16k_arr.clone()].unwrap();
    let ssl_content = ssl_model.run(input_wav16k);
    let ssl_content = ssl_content.unwrap();
    let ssl_content = ssl_content["output"].try_extract_tensor::<f32>().unwrap();

    let sampling_rate: i32 = 32000;
    let hop_length = 640;
    let win_length = 2048;
    let hann_window = hanning(win_length);

    // [1, 768, 239]
    let ssl_content: Array3<f32> = ssl_content.view().slice(s![..,..,..]).to_owned();

    let input_ssl_content = inputs![ssl_content].unwrap();
    let codes = vq_model_latent.run(input_ssl_content);
    let codes = codes.unwrap();
    let codes = codes["output"].try_extract_tensor::<i64>().unwrap();
    let prompt: Array2<i64> = codes.view().slice(s![0,..,..]).to_owned();

    let top_k: Array1<i64> = ndarray::Array1::from(vec![20]);
    let temperature: Array1<f32> = ndarray::Array1::from(vec![0.8]);
    //  合并参考的声音
    let bert: Array3<f32> = ndarray::concatenate(Axis(1), &[bert_features1.view(), bert_features2.view()]).unwrap().insert_axis(Axis(0));

    // 会清空
    let mut _phones_list_unpack1 = phones_list_unpack1.clone();
    let mut _phones_list_unpack2 = phones_list_unpack2.clone();
    _phones_list_unpack1.append(&mut _phones_list_unpack2);

    let all_phoneme_ids: Array2<i64> = Array1::from_vec(_phones_list_unpack1).insert_axis(Axis(0)).mapv(|x| x as i64);
    let text: Array2<i64> = Array1::from_vec(phones_list_unpack2.clone()).insert_axis(Axis(0)).mapv(|x| x as i64);

    let x_example: Array2<f32> = Array2::zeros((all_phoneme_ids.shape()[0], all_phoneme_ids.shape()[1]));

    let start_loop = Instant::now();
    // let first_stage_decoder_input = inputs![all_phoneme_ids,bert,prompt,&top_k,&temperature].unwrap();
    let first_stage_decoder_input = inputs![
        "all_phoneme_ids" => all_phoneme_ids.view(),
        "bert" => bert.view(),
        "prompt" => prompt.view(),
        "top_k" => top_k.view(),
        "temperature" => temperature.view(),
    ].unwrap();
    let start_loop1 = Instant::now();
    let t2s_first_stage_out = t2s_first_stage_decoder.run(first_stage_decoder_input);
    println!("t2s_first_stage time: {}ms", start_loop1.elapsed().as_millis());
    let t2s_first_stage_out = t2s_first_stage_out.unwrap();

    let mut y: Array2<i64> = t2s_first_stage_out["y"].try_extract_tensor::<i64>().unwrap().view().slice(s![..,..]).into_owned();
    let mut k: Array4<f32> = t2s_first_stage_out["k"].try_extract_tensor::<f32>().unwrap().view().slice(s![..,..,..,..]).into_owned();
    let mut v: Array4<f32> = t2s_first_stage_out["v"].try_extract_tensor::<f32>().unwrap().view().slice(s![..,..,..,..]).into_owned();
    let mut y_emb: Array3<f32> = t2s_first_stage_out["y_emb"].try_extract_tensor::<f32>().unwrap().view().slice(s![..,..,..]).into_owned();

    let mut y_example: Array2<f32> = Array2::zeros((1, y_emb.shape()[1]));
    let y_example_0: Array2<f32> = Array2::zeros((1, 1));


    let mut loop_idx = 0;
    for idx in 1..1500 {
        y_example = ndarray::concatenate(Axis(1), &[y_example.view(), y_example_0.view()]).unwrap();
        let xy_attn_mask: Array4<f32> = ndarray::concatenate(Axis(1), &[x_example.view(), y_example.view()])
            .unwrap().insert_axis(Axis(0)).insert_axis(Axis(0));


        let t2s_stage_decoder_input = inputs![
        "y" => y.view(),
        "k" => k.view(),
        "v" => v.view(),
        "y_emb" => y_emb.view(),
        "xy_attn_mask" => xy_attn_mask.view(),
        "top_k" => top_k.view(),
        "temperature" => temperature.view(),
        ].unwrap();

        let start_loop_t2s_stage_decoder = Instant::now();
        let t2s_stage_decoder_out = t2s_stage_decoder.run(t2s_stage_decoder_input);
        println!("stage_decoder: {}ms", start_loop_t2s_stage_decoder.elapsed().as_millis());
        let t2s_stage_decoder_out = t2s_stage_decoder_out.unwrap();

        k = t2s_stage_decoder_out["o_k"].try_extract_tensor::<f32>().unwrap().view().slice(s![..,..,..,..]).into_owned();
        v = t2s_stage_decoder_out["o_v"].try_extract_tensor::<f32>().unwrap().view().slice(s![..,..,..,..]).into_owned();
        y_emb = t2s_stage_decoder_out["o_y_emb"].try_extract_tensor::<f32>().unwrap().view().slice(s![..,..,..]).into_owned();
        let logits: Array1<i64> = t2s_stage_decoder_out["logits"].try_extract_tensor::<i64>().unwrap().view().slice(s![..]).into_owned();
        let samples: Array2<i64> = t2s_stage_decoder_out["samples"].try_extract_tensor::<i64>().unwrap().view().slice(s![..,..]).into_owned();

        y = ndarray::concatenate(Axis(1), &[y.view(), samples.view()]).unwrap();

        let sample = samples.get((0, 0)).unwrap();
        let logit = logits.get(0).unwrap();

        if *logit == 1024 || *sample == 1024 {
            loop_idx = idx;
            break;
        }
    }
    println!("{}ms , loop_idx:{}", start_loop.elapsed().as_millis(), loop_idx);

    *y.get_mut((0, y.shape()[1] - 1)).unwrap() = 0;

    let pred_semantic: Array3<i64> = y.slice(s![..,y.shape()[1]-loop_idx..]).into_owned().insert_axis(Axis(0));

    let y_len = (pred_semantic.shape()[2] * 2) as i64;
    let y_lengths: Array1<i64> = ndarray::Array1::from(vec![y_len]);
    let text_lengths: Array1<i64> = ndarray::Array1::from(vec![text.shape()[0] as i64]);
    let T = (wav32k_arr.shape()[1] - hop_length) / hop_length + 1;
    let refer_mask: Array3<i64> = Array3::ones((pred_semantic.shape()[0], pred_semantic.shape()[1], T));

    let vq_model_input = inputs![
        "pred_semantic"=>pred_semantic.view(),
        "text"=>text.view(),
        "org_audio"=>wav32k_arr.view(),
        "hann_window"=>hann_window.view(),
        "refer_mask"=>refer_mask.view(),
        "y_lengths"=>y_lengths.view(),
        "text_lengths"=>text_lengths.view(),
    ].unwrap();
    let start_vq_model = Instant::now();
    let vq_model_out = vq_model.run(vq_model_input);
    let vq_model_out = vq_model_out.unwrap();
    let start_vq_model2 = Instant::now();

    let mut audio: Array1<f32> = vq_model_out["audio"].try_extract_tensor::<f32>().unwrap().view().slice(s![0,0,..]).into_owned();
    println!("time:{}, audio:{:?}", (start_vq_model2 - start_vq_model).as_millis(), audio.shape());

    let mut audio: Vec<f32> = audio.to_vec();
    let max_audio = {
        let mut max_v = 0.0;
        for &v in &audio {
            let v = num::abs(v);
            if v > max_v {
                max_v = v;
            }
        }
        max_v
    };
    let mut audio_norm = {
        if max_audio > 1.0 {
            let v: Vec<i16> = audio.iter().map(|&x| {
                ((x / max_audio) * 32768.0) as i16
            }).collect();
            v
        } else {
            let v: Vec<i16> = audio.iter().map(|&x| {
                (x * 32768.0) as i16
            }).collect();
            v
        }
    };
    // 保存结果
    FfmpegUtils::decode_data_to_path(&audio_norm, "./make_32k.wav", 32000, 1024).unwrap();

    // SDL 播放
    // let sdl_context = sdl2::init().unwrap();
    // let audio_subsystem = sdl_context.audio().unwrap();
    // let desired_spec = AudioSpecDesired {
    //     freq: Some(sampling_rate),
    //     channels: Some(1),
    //     // mono  -
    //     samples: None, // default sample size
    // };
    //
    // let device = audio_subsystem.open_queue::<i16, _>(None, &desired_spec).unwrap();

    // let t = audio_norm.len() as f32 / sampling_rate as f32;
    // println!("all t:{}s", t);
    //
    // let mut sum_t = 0.0;
    // // 分片给
    // for (i, sample) in audio_norm.chunks(sampling_rate as usize).enumerate() {
    //     device.queue_audio(&sample).unwrap();
    //     if i == 0 {
    //         device.resume();
    //     }
    //     let sample_t = sample.len() as f32 / sampling_rate as f32;
    //     sum_t += sample_t;
    //     println!("all t:{}/{}s ", sum_t, t);
    //     let sample_t = (sample_t * 1000.0) as u64;
    //     std::thread::sleep(Duration::from_millis(sample_t));
    // }
    // device.clear();
}


pub fn infer() {
    let tokenizer_path = Path::new("../data/tokenizer.json");
    let bert_model_path = Path::new("./data/bert_model.onnx");

    // cnhubert_base_path
    let ssl_model_path = Path::new("./data/ssl_model.onnx");

    // sovits_path
    let vq_model_latent_path = Path::new("./data/vq_model_latent.onnx");
    let vq_model_path = Path::new("./data/vq_model.onnx");
    // gpt_path
    let t2s_first_stage_decoder_path = Path::new("./data/t2s_first_stage_decoder.onnx");
    let t2s_stage_decoder_path = Path::new("./data/t2s_stage_decoder.onnx");

    let ch_bert_util = ChBertUtils::init(tokenizer_path.to_str().unwrap());

    let sampling_rate: i32 = 32000;

    let zero_sampling_len = (sampling_rate as f32 * 0.3) as usize;
    let zero_wav: Array1<f32> = Array1::zeros((zero_sampling_len, ));
    println!("zero_wav:{:?}", zero_wav.shape());

    let start = Instant::now();

    // 参考音色音频文件
    let ref_wav_path = "./data/xxx.wav";
    // 参考音色音频对应的文字
    let prompt_text = "我注意到了，没有人说图书馆，我刚到广州就因为广州图书馆住在珠江新城附近。".to_string();

    let wav16k: Vec<i16> = FfmpegUtils::decode_path_to_datas(ref_wav_path, 16000).unwrap();
    let wav32k: Vec<i16> = FfmpegUtils::decode_path_to_datas(ref_wav_path, 32000).unwrap();

    let wav16k: Vec<f32> = wav16k.iter().map(|&x| x as f32 / 32768.0).collect();
    let wav32k: Vec<f32> = wav32k.iter().map(|&x| x as f32 / 32768.0).collect();


    println!("time_t:{} ms ,16k len:{}, 32k len:{}", start.elapsed().as_millis(), wav16k.len(), wav32k.len());

    let wav16k_arr: Array1<f32> = Array1::from_vec(wav16k);
    let wav32k_arr: Array1<f32> = Array1::from_vec(wav32k);

    let wav16k_sum = wav16k_arr.sum();
    let wav32k_sum = wav32k_arr.sum();

    println!("wav16k_sum:{},wav32k_sum:{}", wav16k_sum, wav32k_sum);

    let wav16k_arr: Array2<f32> = ndarray::concatenate(Axis(0), &[wav16k_arr.view(), zero_wav.view()]).unwrap().insert_axis(Axis(0));
    let wav32k_arr: Array2<f32> = wav32k_arr.insert_axis(Axis(0));
    println!("wav16k_arr:{:?} ", wav16k_arr.shape());

    let text = "每个人的理想不一样，扎出来的风筝也不一样。所有的风筝中，要数小音乐家根子的最棒了，那是一架竖琴。让她到天上去好好想想吧！哈，风筝的后脑勺上还拖着一条马尾巴似的长辫子！在地面上，我们一边放线一边跑着，手里的线越放越长，风筝也带着我们的理想越飞越远，越飞越高如果把眼前的一池荷花看作一大幅活的画，那画家的本领可真了不起。".to_string();


    let text_util = TextUtils::init(
        "./data/eng_dict.json",
        "./data/rep_map.json",
        "./data/model.npz",
        "./data/PHRASES_DICT.json",
        "./data/PINYIN_DICT.json",
    ).unwrap();

    let texts = text_util.lang_seg.cut_texts(&text, prompt_text.chars().count());

    println!("texts:{}", texts.join("\n"));

    let start = Instant::now();
    let (mut phones_list, word2ph_list, lang_list, norm_text_list) = text_util.get_cleaned_text_final(&prompt_text);
    println!("time_t2:{} ms", start.elapsed().as_millis());

    let bert_model = ChBertUtils::load_model(bert_model_path.to_str().unwrap()).unwrap();
    let ssl_model = ChBertUtils::load_model(ssl_model_path.to_str().unwrap()).unwrap();
    let vq_model_latent = ChBertUtils::load_model(vq_model_latent_path.to_str().unwrap()).unwrap();
    let t2s_first_stage_decoder = ChBertUtils::load_model(t2s_first_stage_decoder_path.to_str().unwrap()).unwrap();
    let t2s_stage_decoder = ChBertUtils::load_model(t2s_stage_decoder_path.to_str().unwrap()).unwrap();
    let vq_model = ChBertUtils::load_model(vq_model_path.to_str().unwrap()).unwrap();

    let start = Instant::now();
    let (bert_features1, phones_list_unpack1, norm_text_str1) = ChBertUtils::get_bert_features(&ch_bert_util.tokenizer, &bert_model, &mut phones_list, &word2ph_list, &norm_text_list, &lang_list);
    println!("norm_text_str1:{},phones_list_unpack1:{},time_t3:{} ms", norm_text_str1, phones_list_unpack1.len(), start.elapsed().as_millis());

    println!("bert_features1.shape:{:?}", bert_features1.shape());

    for text in texts {
        let (mut phones_list, word2ph_list, lang_list, norm_text_list) = text_util.get_cleaned_text_final(&text);
        let (bert_features2, phones_list_unpack2, norm_text_str2) = ChBertUtils::get_bert_features(&ch_bert_util.tokenizer, &bert_model, &mut phones_list, &word2ph_list, &norm_text_list, &lang_list);

        println!("phones_list_unpack2:{:?}", phones_list_unpack2);
        println!("text:{} ->{}", text, norm_text_str2);

        test_infer_wav(&ssl_model, &vq_model_latent, &t2s_first_stage_decoder, &t2s_stage_decoder, &vq_model, &wav16k_arr, &wav32k_arr, &bert_features1, &bert_features2, &phones_list_unpack1, &phones_list_unpack2);
        break;
    }
}
