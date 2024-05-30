use std::ffi::{c_int, CStr};
use std::ptr;
use std::ptr::NonNull;
use anyhow::{bail, Context};
use cstr::cstr;
use log::info;
use opencv::core::{Mat, MatTraitConst};
use rsmpeg::avcodec::{AVCodec, AVCodecContext, AVCodecRef};
use rsmpeg::avformat::{AVFormatContextOutput, AVIOContextContainer, AVIOContextURL};
use rsmpeg::avutil::{AVFrame, ra};
use rsmpeg::error::RsmpegError;
use rsmpeg::{ffi, UnsafeDerefMut};
use rsmpeg::ffi::{AVPixelFormat_AV_PIX_FMT_BGR24, AVPixelFormat_AV_PIX_FMT_YUV420P, AVRational, SWS_FAST_BILINEAR, sws_freeContext, sws_getContext, sws_scale};

pub fn get_encoder() -> AVCodecRef<'static> {
    let mut encoder = AVCodec::find_encoder(ffi::AVCodecID_AV_CODEC_ID_H264).unwrap();
    if cfg!(target_os = "windows") {
        let _encoder = AVCodec::find_encoder_by_name(cstr!("h264_nvenc"));
        if _encoder.is_some() {
            println!("encoder:h264_nvenc");
            encoder = _encoder.unwrap();
        } else {
            println!("encoder:h264_nvenc error");
        }
    } else if cfg!(target_os = "macos") {
        let _encoder = AVCodec::find_encoder_by_name(cstr!("h264_videotoolbox"));
        if _encoder.is_some() {
            println!("encoder:h264_videotoolbox");
            encoder = _encoder.unwrap();
        } else {
            println!("encoder:h264_videotoolbox error");
        }
    }
    return encoder;
}

pub fn create2(filename: &CStr,
               io_context: Option<AVIOContextContainer>,
               format_name: Option<&CStr>) -> rsmpeg::error::Result<AVFormatContextOutput> {
    let mut output_format_context = ptr::null_mut();

    unsafe {
        if format_name.is_some() {
            let format_str = format_name.unwrap();
            ffi::avformat_alloc_output_context2(
                &mut output_format_context,
                ptr::null_mut(),
                format_str.as_ptr(),
                filename.as_ptr(),
            )
        } else {
            ffi::avformat_alloc_output_context2(
                &mut output_format_context,
                ptr::null_mut(),
                ptr::null_mut(),
                filename.as_ptr(),
            )
        }
    };

    let mut output_format_context =
        unsafe { AVFormatContextOutput::from_raw(NonNull::new(output_format_context).unwrap()) };

    if output_format_context.oformat().flags & ffi::AVFMT_NOFILE as i32 == 0 {
        let mut io_context = io_context.unwrap_or_else(|| {
            AVIOContextContainer::Url(AVIOContextURL::open(filename, ffi::AVIO_FLAG_WRITE).unwrap())
        });
        unsafe {
            output_format_context.deref_mut().pb = match &mut io_context {
                AVIOContextContainer::Url(ctx) => ctx.as_mut_ptr(),
                AVIOContextContainer::Custom(ctx) => ctx.as_mut_ptr(),
            };
        }
        output_format_context.io_context = Some(io_context);
    }

    Ok(output_format_context)
}


/// encode -> write_frame
pub(crate) fn encode_write_frame(
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

/// Send an empty packet to the `encode_context` for packet flushing.
pub(crate) fn flush_encoder(
    encode_context: &mut AVCodecContext,
    output_format_context: &mut AVFormatContextOutput,
    out_stream_index: usize,
) -> anyhow::Result<()> {
    if encode_context.codec().capabilities & ffi::AV_CODEC_CAP_DELAY as i32 == 0 {
        return Ok(());
    }
    encode_write_frame(
        None,
        encode_context,
        output_format_context,
        out_stream_index,
    )?;
    Ok(())
}


pub(crate) unsafe fn CvMatToAVFrame(input_mat: &Mat, out_avframe: &mut AVFrame) {
    let image_width = input_mat.size().unwrap().width;
    let image_height = input_mat.size().unwrap().height;
    let mut cvLinesizes: [c_int; 1] = [1];
    cvLinesizes[0] = input_mat.step1(0).unwrap() as c_int;

    let mut openCVBGRToAVFrameSwsContext = sws_getContext(
        image_width,
        image_height,
        AVPixelFormat_AV_PIX_FMT_BGR24,
        image_width,
        image_height,
        AVPixelFormat_AV_PIX_FMT_YUV420P,
        SWS_FAST_BILINEAR as c_int,
        ptr::null_mut(),
        ptr::null_mut(),
        ptr::null_mut(),
    );

    sws_scale(openCVBGRToAVFrameSwsContext,
              &input_mat.data(),
              cvLinesizes.as_ptr(),
              0,
              image_height,
              out_avframe.data.as_ptr(),
              out_avframe.linesize.as_ptr());
    if (openCVBGRToAVFrameSwsContext != ptr::null_mut()) {
        sws_freeContext(openCVBGRToAVFrameSwsContext);
        openCVBGRToAVFrameSwsContext = ptr::null_mut();
    }
}

// 只是创建了一个视频流
pub(crate) fn open_output_file(
    filename: &CStr,
    width: i32,
    height: i32,
    crf: i32,
    time_base: AVRational,
    framerate: AVRational,
) -> anyhow::Result<(AVFormatContextOutput, AVCodecContext), RsmpegError> {
    let bit_rate: i64 = {
        if crf <= 10 {
            8000000
        } else if crf <= 20 {
            4000000
        } else {
            1000000
        }
    };

    fn set_encode_context(encode_context: &mut AVCodecContext,
                          output_format_context: &AVFormatContextOutput,
                          bit_rate: i64,
                          width: i32,
                          height: i32,
                          time_base: AVRational,
                          framerate: AVRational,
    ) {
        encode_context.set_bit_rate(bit_rate);
        encode_context.set_width(width);
        encode_context.set_height(height);
        encode_context.set_time_base(time_base);
        encode_context.set_framerate(framerate);
        encode_context.set_gop_size(50);
        // encode_context.set_max_b_frames(0);
        encode_context.set_pix_fmt(ffi::AVPixelFormat_AV_PIX_FMT_YUV420P);

        if output_format_context.oformat().flags & ffi::AVFMT_GLOBALHEADER as i32 != 0 {
            encode_context.set_flags(encode_context.flags | ffi::AV_CODEC_FLAG_GLOBAL_HEADER as i32);
        }
    }

    // println!("bit_rate:{}",bit_rate);

    let mut encoder = get_encoder();
    let mut encode_context = AVCodecContext::new(&encoder);


    let _output_format_context = create2(filename, None, None);
    if _output_format_context.is_err() {
        return Err(_output_format_context.err().unwrap());
    }
    let mut output_format_context = _output_format_context.unwrap();

    set_encode_context(&mut encode_context, &output_format_context, bit_rate, width, height, time_base, framerate);

    let open = encode_context.open(None);
    // 失败则用标准CPU h264
    if open.is_err() {
        encoder = AVCodec::find_encoder(ffi::AVCodecID_AV_CODEC_ID_H264).unwrap();
        encode_context = AVCodecContext::new(&encoder);
        set_encode_context(&mut encode_context, &output_format_context, bit_rate, width, height, time_base, framerate);
        let open = encode_context.open(None);
        if open.is_err() {
            return Err(open.err().unwrap());
        }
    }

    // add video stream
    {
        let mut out_stream = output_format_context.new_stream();
        out_stream.set_codecpar(encode_context.extract_codecpar());
        out_stream.set_time_base(encode_context.time_base);
    }

    output_format_context.dump(0, filename).unwrap();

    let rhead = output_format_context.write_header(&mut None);
    if rhead.is_err() {
        return Err(rhead.err().unwrap());
    }

    Ok((output_format_context, encode_context))
}