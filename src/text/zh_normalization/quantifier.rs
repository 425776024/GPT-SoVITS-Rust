use std::collections::HashMap;
// use regex::{Captures, Regex};
use fancy_regex::{Captures, Regex};
use crate::tts_sovits::text::zh_normalization::num::NumUtil;

pub struct Quantifier {
    pub RE_TEMPERATURE: Regex,
    pub measure_dict: HashMap<String, String>,
    pub measure_dict_keys: Vec<String>,
    pub num_util: NumUtil,
}

impl Quantifier {
    pub fn init() -> Self {
        let RE_TEMPERATURE = Regex::new(r"(-?)(\d+(\.\d+)?)(°C|℃|度|摄氏度)").unwrap();
        let mut measure_dict: HashMap<String, String> = HashMap::new();
        // 顺序有先后
        let mk = ["cm2", "cm²", "cm3", "cm³", "cm", "db", "ds", "kg", "km", "m2", "m²", "m³", "m3", "ml", "m", "mm", "s"];
        let mv = ["平方厘米", "平方厘米", "立方厘米", "立方厘米", "厘米", "分贝", "毫秒", "千克", "千米", "平方米", "平方米", "立方米", "立方米", "毫升", "米", "毫米", "秒"];

        let mut measure_dict_keys = vec![];
        for i in 0..mk.len() {
            let k = mk[i];
            let v = mv[i];
            measure_dict.insert(k.to_string(), v.to_string());
            measure_dict_keys.push(k.to_string());
        }


        let num_util = NumUtil::init();
        Quantifier { RE_TEMPERATURE, measure_dict, measure_dict_keys, num_util }
    }

    pub fn replace_temperature(&self, sentence: String) -> String {
        let replacement = |caps: &Captures| -> String{
            let sign = caps.get(1).map_or("", |m| m.as_str());
            let temperature: Option<&str> = caps.get(2).map_or(None, |m| Some(m.as_str()));
            let unit: Option<&str> = caps.get(3).map_or(None, |m| Some(m.as_str()));
            let sign = {
                if sign != "" {
                    "零下"
                } else {
                    ""
                }
            };
            let temperature = self.num_util.num2str(temperature.unwrap().to_string());

            let unit = {
                if unit.is_some() && unit.unwrap() == "摄氏度" {
                    "摄氏度"
                } else {
                    "度"
                }
            };
            let result = format!("{}{}{}", sign, temperature, unit);
            result
        };

        let caps = self.RE_TEMPERATURE.replace_all(&sentence, replacement).to_string();
        caps
    }

    pub fn replace_measure(&self, sentence: String) -> String {
        let mut sentence = sentence;
        for q_k in &self.measure_dict_keys {
            if sentence.contains(q_k) {
                sentence = sentence.replace(q_k, &self.measure_dict.get(q_k).unwrap());
            }
        }
        sentence
    }
}