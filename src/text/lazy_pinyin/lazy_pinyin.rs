use std::collections::HashMap;
use std::fs;
use pinyin::ToPinyin;
use regex::Regex;
use crate::tts_sovits::text::lazy_pinyin::mmseg::MMSeg;
use crate::tts_sovits::text::lazy_pinyin::style::{convert_styles, Style};

pub struct LazyPinyin {
    neutral_tone_with_five: bool,
    mmseg: MMSeg,
    RE_HANS: Regex,
    phrases_dict: HashMap<String, Vec<Vec<String>>>,
    pinyin_dict: HashMap<String, String>,
}


fn _remove_dup_items(lst: &Vec<String>, remove_empty: bool) -> Vec<String> {
    let mut new_lst = vec![];
    for item in lst {
        if remove_empty && item == "" {
            continue;
        }
        if !new_lst.contains(item) {
            new_lst.push(item.clone());
        }
    }
    new_lst
}

fn _remove_dup_and_empty(lst_list: Vec<Vec<String>>) -> Vec<Vec<String>> {
    let mut new_lst_list: Vec<Vec<String>> = vec![];
    for lst in lst_list {
        let lst = _remove_dup_items(&lst, true);
        if !lst.is_empty() {
            new_lst_list.push(lst);
        } else {
            new_lst_list.push(vec!["".to_string()]);
        }
    }

    new_lst_list
}


impl LazyPinyin {
    pub fn init(phrases_dict_path: &str, pinyin_dict_path: &str) -> Result<Self, String> {
        let file_op = fs::File::open(phrases_dict_path).unwrap();
        let file_op2 = fs::File::open(pinyin_dict_path).unwrap();

        let phrases_dict: HashMap<String, Vec<Vec<String>>> = serde_json::from_reader(&file_op).unwrap();
        let pinyin_dict: HashMap<String, String> = serde_json::from_reader(&file_op2).unwrap();

        let mmseg = MMSeg::init(true, &phrases_dict);

        let RE_HANS = Regex::new(r"^(?:[\u3007\ue815-\ue864\ufa18\u3400-\u4dbf\u4e00-\u9fff\uf900-\ufaff\\U00020000-\\U0002A6DF\\U0002A703-\\U0002B73F\\U0002B740-\\U0002B81D\\U0002B825-\\U0002BF6E\\U0002C029-\\U0002CE93\\U0002D016\\U0002D11B-\\U0002EBD9\\U0002F80A-\\U0002FA1F\\U000300F7-\\U00031288\\U00030EDD\\U00030EDE'])+$").unwrap();

        Ok(LazyPinyin { neutral_tone_with_five: true, mmseg, RE_HANS, phrases_dict, pinyin_dict })
    }

    pub fn lazy_pinyin(&self, hans: &str,
                       style: Style, strict: bool) -> Vec<Vec<String>> {
        let han_list = self.seg(hans, &self.phrases_dict);
        let mut pys = vec![];
        for words in han_list {
            let mut p = self.convert(&words, style, strict);
            // 转换失败，启用lib的
            if p.is_empty() {
                let mut initials: Vec<String> = vec![];
                let mut finals: Vec<String> = vec![];
                for (i, p) in words.as_str().to_pinyin().enumerate() {
                    if p.is_some() {
                        let p = p.unwrap();
                        let py = p.initials();
                        let py2 = p.finals_with_tone_num_end();

                        let py2_len = py2.chars().count();
                        let py2_tone = {
                            let e = py2.chars().nth(py2_len - 1);
                            if e.is_some() {
                                if !e.unwrap().is_numeric() {
                                    "5".to_string()
                                } else {
                                    "".to_string()
                                }
                            } else {
                                "".to_string()
                            }
                        };
                        let py2 = format!("{}{}", py2, py2_tone);
                        initials.push(py.to_string());
                        finals.push(py2.to_string());
                    } else {
                        let wc = words.chars().nth(i).unwrap();
                        initials.push(wc.to_string());
                        finals.push(wc.to_string());
                    }
                }
                if style == Style::INITIALS {
                    p.push(initials);
                } else if style == Style::FINALS_TONE3 {
                    p.push(finals);
                }
                // p = vec![vec![words]];
            }
            pys.append(&mut p);
        }
        pys
    }

    /// 根据参数把汉字转成相应风格的拼音结果。
    //
    //         :param words: 汉字字符串
    //         :type words: unicode
    //         :param style: 拼音风格
    //         :param heteronym: 是否启用多音字
    //         :type heteronym: bool
    //         :param errors: 如果处理没有拼音的字符
    //         :param strict: 只获取声母或只获取韵母相关拼音风格的返回结果
    //                        是否严格遵照《汉语拼音方案》来处理声母和韵母，
    //                        详见 :ref:`strict`
    //         :type strict: bool
    //         :return: 按风格转换后的拼音结果
    //         :rtype: list
    fn convert(&self, words: &str, style: Style, strict: bool) -> Vec<Vec<String>> {
        let mut pys = vec![];
        if self.RE_HANS.is_match(words) {
            pys = self._phrase_pinyin(words, style)
        }
        pys = convert_styles(pys, words, style, strict);
        let pys = _remove_dup_and_empty(pys);
        pys
    }

    fn _phrase_pinyin(&self, phrase: &str, style: Style) -> Vec<Vec<String>> {
        let mut pinyin_list = vec![];
        if self.phrases_dict.contains_key(phrase) {
            let mut py = self.phrases_dict.get(phrase).unwrap().clone();
            pinyin_list.append(&mut py);
        } else {
            for han in phrase.chars() {
                let mut py = self._single_pinyin(&han.to_string(), style);
                pinyin_list.append(&mut py);
            }
        }
        pinyin_list
    }

    fn _single_pinyin(&self, han: &String, style: Style) -> Vec<Vec<String>> {
        let mut pinyin_list: Vec<Vec<String>> = vec![];
        if self.pinyin_dict.contains_key(han) {
            let pys = self.pinyin_dict.get(han).unwrap().split(",").map(|x| x.to_string()).collect();
            pinyin_list.push(pys);
            return pinyin_list;
        } else {
            // todo
        }

        pinyin_list
    }

    fn handle_nopinyin() {}

    fn seg(&self, hans: &str, phrases_dict: &HashMap<String, Vec<Vec<String>>>) -> Vec<String> {
        self.mmseg.seg(hans, phrases_dict)
    }
}

#[test]
fn testlz() {
    let m = LazyPinyin::init(
        "/Users/jxinfa/RustroverProjects/rs_lazy_pinyin/datas/PHRASES_DICT.json",
        "/Users/jxinfa/RustroverProjects/rs_lazy_pinyin/datas/PINYIN_DICT.json",
    ).unwrap();
    let v = m.lazy_pinyin("你好", Style::FINALS_TONE3, true);
    println!("{:?}", v);
}