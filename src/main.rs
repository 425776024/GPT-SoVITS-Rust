mod text_utils;
mod text;
mod bert_utils;
mod ffmpeg_utils;

use english_numbers;
use crate::bert_utils::{infer};
use num_traits::sign::Signed;

fn main() {
    infer();
}
