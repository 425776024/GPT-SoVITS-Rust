// use regex::Regex;
use fancy_regex::{Captures, Regex};
use crate::tts_sovits::text::zh_normalization::num::NumUtil;

pub struct Phonecode {
    pub RE_MOBILE_PHONE: Regex,
    pub RE_TELEPHONE: Regex,
    pub RE_NATIONAL_UNIFORM_NUMBER: Regex,
    pub num_util: NumUtil,
}

impl Phonecode {
    pub fn init() -> Self {
        let RE_MOBILE_PHONE = Regex::new(r"[?<!\d]((\+?86 ?)?1([38]\d|5[0-35-9]|7[678]|9[89])\d{8})[?!\d]").unwrap();
        let RE_TELEPHONE = Regex::new(r"[?<!\d]((0(10|2[1-3]|[3-9]\d{2})-?)?[1-9]\d{6,7})[?!\d]").unwrap();
        // 全国统一的号码400开头
        let RE_NATIONAL_UNIFORM_NUMBER = Regex::new(r"(400)(-)?\d{3}(-)?\d{4}").unwrap();
        let num_util = NumUtil::init();
        Phonecode { RE_MOBILE_PHONE, RE_TELEPHONE, RE_NATIONAL_UNIFORM_NUMBER, num_util }
    }

    fn phone2str(&self, phone_string: String, mobile: bool) -> String {
        let mut results: Vec<String> = vec![];
        let sp_parts_opt = phone_string.strip_prefix("+");
        if mobile && sp_parts_opt.is_some() {
            let sp_parts: Vec<&str> = sp_parts_opt.unwrap().split_whitespace().collect();
            for sp in sp_parts {
                let d = self.num_util.verbalize_digit(sp.to_string(), true);
                results.push(d);
            }
        } else {
            let sil_parts: Vec<&str> = phone_string.split("-").collect();
            for sp in sil_parts {
                let d = self.num_util.verbalize_digit(sp.to_string(), true);
                results.push(d);
            }
        }

        let result = results.join("，");
        result
    }


    pub fn _replace(&self, phone_string: String, re: &Regex, mobile: bool) -> String {
        let replacement = |caps: &Captures| -> String{
            let mobile_str: Option<&str> = caps.get(0).map_or(None, |m| Some(m.as_str()));

            if mobile_str.is_some() {
                let mobile_str = mobile_str.unwrap().to_string();
                self.phone2str(mobile_str, mobile)
            } else {
                "".to_string()
            }
        };

        let caps = re.replace_all(&phone_string, replacement).to_string();
        caps
    }

    pub fn replace_phone(&self, phone_string: String) -> String {
        self._replace(phone_string, &self.RE_TELEPHONE, false)
    }

    pub fn replace_phone2(&self, phone_string: String) -> String {
        self._replace(phone_string, &self.RE_NATIONAL_UNIFORM_NUMBER, false)
    }


    pub fn replace_mobile(&self, phone_string: String) -> String {
        self._replace(phone_string, &self.RE_MOBILE_PHONE, true)
    }
}