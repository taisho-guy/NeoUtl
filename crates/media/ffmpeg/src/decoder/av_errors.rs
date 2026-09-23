use ffmpeg_sys_next as sys;

pub(crate) fn averror_eagain() -> i32 {
    -(libc::EAGAIN as i32)
}

pub(crate) fn averror_eof() -> i32 {
    sys::AVERROR_EOF
}

pub(crate) fn ignore_send_packet_result(result: i32) -> bool {
    result >= 0
        || result == averror_eagain()
        || result == averror_eof()
        || result == sys::AVERROR_INVALIDDATA
        || result == -(libc::EINVAL as i32)
}
