use std::cmp::max;
use anyhow::{bail, Context, Result};
use rsmpeg::{
    avcodec::{AVCodecContext, AVCodecParserContext, AVPacket},
    avformat::AVFormatContextInput,
    avutil::{
        get_bytes_per_sample, get_packed_sample_fmt, get_sample_fmt_name, is_planar, AVFrame,
        AVSampleFormat,
    },
    error::RsmpegError,
    ffi,
};
use std::ffi::{c_char, c_int, CStr, CString};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::{ptr, slice};
use std::ptr::{NonNull, null};
use std::slice::from_raw_parts;
use dasp::{ring_buffer, signal, Signal};
use dasp::interpolate::linear::Linear;
use dasp::interpolate::sinc::Sinc;
use hound::{SampleFormat, WavReader, WavSpec};
use log::info;
use rsmpeg::avcodec::{AVCodec, AVCodecRef};
use rsmpeg::avformat::AVFormatContextOutput;
use rsmpeg::avutil::{av_get_channel_layout_nb_channels, av_get_default_channel_layout, AVAudioFifo, AVSamples};
use rsmpeg::swresample::SwrContext;
use rsmpeg::ffi::{AV_CH_LAYOUT_NATIVE, av_get_channel_layout, av_rescale_rnd, av_samples_copy, AVRational, AVRounding, swr_get_delay};
use soundtouch::{Setting, SoundTouch};


pub struct FfmpegUtils {}


impl FfmpegUtils {
    /// 音频重采样器：乱七八糟的格式转成统一的 1 channel
    pub(crate) fn init_audio_resampler(
        decode_context: &mut AVCodecContext,
        encode_context: &mut AVCodecContext,
    ) -> Result<(bool, SwrContext), String>
    {
        // channel_layout为0，如果此时音频数据转换的时候，需要通道布局的参数。
        // 如果直接将解码器的channel_layout做参数给swr_alloc_set_opts()肯定会出错
        if decode_context.channel_layout == 0 {
            let mut c_str = CString::new("stereo").unwrap();
            // 如果channels通道个数为1，那么强制通道布局为AV_CH_LAYOUT_MONO，
            // 如果channels通道个数为2，那么强制通道布局为AV_CH_LAYOUT_STEREO。
            if decode_context.channels == 1 {
                c_str = CString::new("mono").unwrap();
            } else if decode_context.channels == 2 {
                c_str = CString::new("stereo").unwrap();
            }
            let c_world: *const c_char = c_str.as_ptr() as *const c_char;
            unsafe {
                decode_context.set_channel_layout(av_get_channel_layout(c_world));
            }
        }
        let mut resample_context = SwrContext::new(
            av_get_default_channel_layout(encode_context.channels),
            encode_context.sample_fmt,
            encode_context.sample_rate,
            av_get_default_channel_layout(decode_context.channels),
            decode_context.sample_fmt,
            decode_context.sample_rate,
        ).context("Could not allocate resample context").unwrap();

        let init_res = resample_context
            .init()
            .context("Could not open resample context");
        if init_res.is_err() {
            return Err("Could not open resample context".to_string());
        }
        let is_same = {
            if decode_context.channels == encode_context.channels
                && decode_context.sample_rate == encode_context.sample_rate
                && decode_context.sample_fmt == encode_context.sample_fmt
                && decode_context.channel_layout == encode_context.channel_layout
            {
                true
            } else {
                false
            }
        };

        Ok((is_same, resample_context))
    }

    fn encode_write_frame(
        frame_after: Option<&AVFrame>,
        encode_context: &mut AVCodecContext,
        output_format_context: &mut AVFormatContextOutput,
        out_stream_index: usize,
    ) -> anyhow::Result<()> {
        encode_context
            .send_frame(frame_after)
            .context("Encode frame failed.")?;
        loop {
            let mut packet = match encode_context.receive_packet() {
                Ok(packet) => packet,
                Err(RsmpegError::EncoderDrainError) | Err(RsmpegError::EncoderFlushedError) => break,
                Err(e) => bail!(e),
            };

            packet.set_stream_index(out_stream_index as i32);
            packet.rescale_ts(
                encode_context.time_base,
                output_format_context
                    .streams()
                    .get(out_stream_index)
                    .unwrap()
                    .time_base,
            );

            match output_format_context.interleaved_write_frame(&mut packet) {
                Ok(()) => Ok(()),
                Err(RsmpegError::InterleavedWriteFrameError(-22)) => Ok(()),
                Err(e) => Err(e),
            }
                .context("Interleaved write frame failed.")?;
        }

        Ok(())
    }

    fn flush_encoder(
        encode_context: &mut AVCodecContext,
        output_format_context: &mut AVFormatContextOutput,
        out_stream_index: usize,
    ) -> anyhow::Result<()> {
        if encode_context.codec().capabilities & ffi::AV_CODEC_CAP_DELAY as i32 == 0 {
            return Ok(());
        }
        FfmpegUtils::encode_write_frame(
            None,
            encode_context,
            output_format_context,
            out_stream_index,
        )?;
        Ok(())
    }

    fn get_audio_decoder(audio_path: &str) -> Result<(AVCodecRef, AVCodecContext, usize, AVFormatContextInput)> {
        let audio_path = CString::new(audio_path).unwrap();
        let (decoder, mut decode_context, stream_index, mut input_format_context) = {
            let mut input_format_context = AVFormatContextInput::open(&audio_path, None, &mut None)
                .context("Open audio file failed.")?;
            let (stream_index, decoder) = input_format_context
                .find_best_stream(ffi::AVMediaType_AVMEDIA_TYPE_AUDIO)
                .context("Find best stream failed.")?
                .context("Cannot find audio stream in this file.")?;
            let mut decode_context = AVCodecContext::new(&decoder);
            decode_context
                .apply_codecpar(
                    &input_format_context
                        .streams()
                        .get(stream_index)
                        .unwrap()
                        .codecpar(),
                )
                .context("Apply codecpar failed.")?;

            info!("decode_context.channels:{}", decode_context.channels);
            info!("decode_context.sample_rate:{}", decode_context.sample_rate);
            info!("decode_context.max_samples:{}", decode_context.max_samples);

            decode_context
                .open(None)
                .context("Open codec context failed.")?;

            // info!("input_format_context.dump:");
            // input_format_context.dump(stream_index, &audio_path)?;

            (decoder, decode_context, stream_index, input_format_context)
        };

        Ok((decoder, decode_context, stream_index, input_format_context))
    }

    /// pcm16 audio encode
    pub(crate) fn get_pcm16_encode(output_format_context: Option<&AVFormatContextOutput>, sr: i32) -> AVCodecContext {
        let encode_context = unsafe {
            let encoder = AVCodec::find_encoder(ffi::AVCodecID_AV_CODEC_ID_PCM_S16LE).unwrap();
            let mut encode_context = AVCodecContext::new(&encoder);

            if output_format_context.is_some() {
                if output_format_context.unwrap().oformat().flags & ffi::AVFMT_GLOBALHEADER as i32 != 0 {
                    encode_context.set_flags(encode_context.flags | ffi::AV_CODEC_FLAG_GLOBAL_HEADER as i32);
                }
            }

            encode_context.set_sample_fmt(ffi::AVSampleFormat_AV_SAMPLE_FMT_S16);
            encode_context.set_sample_rate(sr);
            encode_context.set_bit_rate((sr * 8 * 1) as i64);

            let c_str = CString::new("mono").unwrap();
            let c_world: *const c_char = c_str.as_ptr() as *const c_char;

            encode_context.set_time_base(AVRational { num: 1, den: sr });

            let cl = av_get_channel_layout(c_world);
            encode_context.set_channel_layout(cl);
            let cls = av_get_channel_layout_nb_channels(encode_context.channel_layout);
            encode_context.set_channels(cls);

            encode_context.open(None).unwrap();
            encode_context
        };
        encode_context
    }


    /// 音频保存为文件
    pub fn decode_data_to_path(audio_data: &Vec<i16>,
                               out_file_path: &str,
                               sample_rate: i32,
                               nb_samples: i32,
    ) -> Result<()> {
        let out_file_path = CString::new(out_file_path).unwrap();

        let mut output_format_context = AVFormatContextOutput::create(&out_file_path, None).context("Open audio file failed.")?;

        let mut encode_context = FfmpegUtils::get_pcm16_encode(Some(&output_format_context), sample_rate);
        {
            let mut out_stream = output_format_context.new_stream();
            out_stream.set_codecpar(encode_context.extract_codecpar());
        }
        info!("output_format_context.dump:");
        output_format_context.dump(0, out_file_path.as_c_str())?;
        output_format_context.write_header(&mut None)?;


        let mut PTS = 0;

        let mut frame = AVFrame::new();
        frame.set_format(encode_context.sample_fmt);
        frame.set_channel_layout(encode_context.channel_layout);
        frame.set_sample_rate(encode_context.sample_rate);
        frame.set_nb_samples(nb_samples);
        frame.alloc_buffer().unwrap();

        for (i, sample) in audio_data.chunks(nb_samples as usize).enumerate() {
            let data_ptr = frame.data;
            let data_len = sample.len();
            let linesize = frame.linesize;

            let y_data: &mut [i16] = unsafe { std::slice::from_raw_parts_mut(data_ptr[0] as *mut i16, data_len) };

            for y in 0..data_len {
                let v = sample[y];
                y_data[y] = v;
            }

            frame.set_pts(PTS);
            PTS += frame.nb_samples as i64;

            encode_context
                .send_frame(Some(&frame))
                .context("Encode frame failed.")?;

            loop {
                let mut en_packet = match encode_context.receive_packet() {
                    Ok(en_packet) => en_packet,
                    Err(RsmpegError::EncoderDrainError) | Err(RsmpegError::EncoderFlushedError) => break,
                    Err(e) => break,
                };
                en_packet.set_stream_index(0 as i32);
                en_packet.rescale_ts(
                    encode_context.time_base,
                    output_format_context
                        .streams()
                        .get(0)
                        .unwrap()
                        .time_base,
                );
                output_format_context.write_frame(&mut en_packet)?;
            }
        }


        FfmpegUtils::flush_encoder(&mut encode_context, &mut output_format_context, 0).unwrap();
        output_format_context.write_trailer().unwrap();

        Ok(())
    }


    /// 读取音频为一维数组 any audio file -> 1 channel sr pcm16 vec
    pub fn decode_path_to_datas(audio_path: &str, sr_to: i32) -> Result<Vec<i16>, String> {
        let oepn_ok = FfmpegUtils::get_audio_decoder(audio_path);
        if oepn_ok.is_err() {
            return Err(oepn_ok.err().unwrap().to_string());
        }
        let (_, mut decode_context, stream_index, mut input_format_context) = oepn_ok.unwrap();

        // 保持采样率不变：否则可能会噪音
        let mut encode_context = FfmpegUtils::get_pcm16_encode(None, decode_context.sample_rate);
        // 保持采样率不变，转成PCM，然后在用dasp转换采样率
        let (is_same, mut audio_resample_context) = FfmpegUtils::init_audio_resampler(&mut decode_context, &mut encode_context).unwrap();

        let mut audio_datas: Vec<i16> = vec![];
        let mut sr_audio_datas: Vec<i16> = vec![];

        loop {
            let mut packet = match input_format_context.read_packet() {
                Ok(Some(x)) => x,
                Ok(None) => break,
                Err(e) => break,
            };

            if packet.stream_index as usize != stream_index {
                continue;
            }

            decode_context.send_packet(Some(&packet)).unwrap();

            loop {
                let frame = match decode_context.receive_frame() {
                    Ok(frame) => Some(frame),
                    Err(RsmpegError::DecoderDrainError) | Err(RsmpegError::DecoderFlushedError) => None,
                    Err(e) => None,
                };

                if frame.is_some() {
                    let _frame = frame.unwrap();
                    let dst_nb_samples = unsafe {
                        // swr_ctx中缓存的采样点数量
                        let delay = swr_get_delay(audio_resample_context.as_mut_ptr(), _frame.sample_rate as i64);
                        //重新计算输出音频帧一帧采样点数量
                        let src_nb_samples = _frame.nb_samples as i64;

                        let dst_rate = encode_context.sample_rate;
                        let src_rate = decode_context.sample_rate;

                        av_rescale_rnd(delay + src_nb_samples, dst_rate as i64, src_rate as i64, 1)
                    };

                    let mut output_samples = AVSamples::new(
                        encode_context.channels,
                        dst_nb_samples as i32,
                        encode_context.sample_fmt,
                        0,
                    )
                        .context("Create samples buffer failed.").unwrap();

                    unsafe {
                        audio_resample_context.convert(
                            &mut output_samples,
                            _frame.extended_data as *const _,
                            _frame.nb_samples,
                        ).unwrap();
                    }

                    let data_ptr = output_samples.audio_data[0];

                    let data_len = output_samples.nb_samples;

                    // AVSampleFormat_AV_SAMPLE_FMT_S16 -> 是i16 所以转成i64
                    let data_slice = unsafe { slice::from_raw_parts(data_ptr as *const i16, data_len as usize) };
                    let mut datas = data_slice.to_vec();

                    audio_datas.append(&mut datas);

                    if audio_datas.len() > 16000 * 5 {
                        let mut sr_wavs = FfmpegUtils::pcm_hz_to_hz(&audio_datas, decode_context.sample_rate as f64, sr_to as f64);
                        sr_audio_datas.append(&mut sr_wavs);
                        audio_datas = vec![];
                    }
                } else {
                    break;
                }
            }
        }

        if audio_datas.len() > 0 {
            let mut sr_wavs = FfmpegUtils::pcm_hz_to_hz(&audio_datas, decode_context.sample_rate as f64, sr_to as f64);
            sr_audio_datas.append(&mut sr_wavs);
            audio_datas = vec![];
        }

        Ok(sr_audio_datas)
    }

    // PCM数据的采样率转换（重采样）
    pub fn pcm_hz_to_hz(audio_datas: &Vec<i16>, from_hz: f64, to_hz: f64) -> Vec<i16> {
        if from_hz == to_hz {
            return audio_datas.clone();
        }
        let mut source = signal::from_iter(audio_datas.iter().cloned());
        let a = source.next();
        let b = source.next();
        let interp = Linear::new(a, b);


        let mut wavs = vec![];
        for frame in source.from_hz_to_hz(interp, from_hz as f64, to_hz).until_exhausted() {
            wavs.push(frame);
        }

        wavs
    }

    /// 变速0.5 - 2.0
    fn _sound_touch(data_i16: &Vec<i16>, sr: u32, atempo: f64) -> Vec<f32> {
        let wavs: Vec<f32> = data_i16.iter().map(|&x| x as f32 / 32768.0).collect();
        if atempo == 1.0 {
            return wavs;
        } else {
            let mut soundtouch = SoundTouch::new();
            soundtouch.set_sample_rate(sr as u32)
                .set_pitch_semitones(1)
                .set_tempo(atempo)
                .set_channels(1);

            soundtouch.set_setting(Setting::UseQuickseek, 0);
            soundtouch.set_setting(Setting::UseAaFilter, 1);
            soundtouch.set_setting(Setting::SequenceMs, 30);
            soundtouch.set_setting(Setting::SeekwindowMs, 15);
            soundtouch.set_setting(Setting::OverlapMs, 8);
            let output_samples = soundtouch.generate_audio(&wavs);
            return output_samples;
        }
    }

    /// 变速0.3 - 2.0
    pub fn sound_touch(data_i16: &Vec<i16>, sr: u32, atempo: f64) -> Vec<f32> {
        if atempo >= 0.5 {
            let wavs: Vec<f32> = FfmpegUtils::_sound_touch(data_i16, sr, atempo);
            return wavs;
        } else {
            // 0.3-> 0.66666 + 0.5
            let atempo = atempo * 2.0;
            let data_x2 = FfmpegUtils::sound_touch(&data_i16, sr, atempo);
            let wavs: Vec<i16> = data_x2.iter().map(|&x| (x * 32768.0) as i16).collect();
            let data_x5 = FfmpegUtils::sound_touch(&wavs, sr, 0.5);
            return data_x5;
        }
    }
}


#[test]
fn test_datas() {
    // let ref_wav_path = "/Users/jxinfa/PycharmProjects/sovits_infer/data/leilei.wav";
    let ref_wav_path = "/Users/jxinfa/Downloads/tmp49dvo0e1.wav";
    // let wav32k: Vec<i16> = FfmpegUtils::decode_audio_to_datas(ref_wav_path, 32000).unwrap();
    let sr_to = 16000;
    let wav16k: Vec<i16> = FfmpegUtils::decode_path_to_datas(ref_wav_path, sr_to).unwrap();

    println!("wav16k:{}", wav16k.len() as f32 / sr_to as f32);
    // FfmpegUtils::decode_data_to_path(&wav32k, "/Users/jxinfa/PycharmProjects/sovits_infer/data/leilei_make_32k.wav", 32000, 1024).unwrap();
    FfmpegUtils::decode_data_to_path(&wav16k, "/Users/jxinfa/PycharmProjects/sovits_infer/data/leilei_make_16k.wav", sr_to, 1024).unwrap();
}
