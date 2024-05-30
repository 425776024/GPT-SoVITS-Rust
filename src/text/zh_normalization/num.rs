use std::collections::HashMap;
// use regex::{Captures, Regex};
use fancy_regex::{Captures, Regex};

pub struct NumUtil {
    pub DIGITS: HashMap<String, char>,
    pub UNITS: HashMap<usize, char>,
    pub RE_FRAC: Regex,
    pub RE_PERCENTAGE: Regex,
    pub RE_RANGE: Regex,
    pub RE_NUMBER: Regex,
    pub RE_INTEGER: Regex,
    pub RE_DECIMAL_NUM: Regex,
    pub RE_DEFAULT_NUM: Regex,
    pub RE_POSITIVE_QUANTIFIERS: Regex,
}

#[test]
fn num_test0() {
    let num_util = NumUtil::init();
    let s = num_util.replace_negative_num("-2014".to_string());
    println!("s:{:?}", s);
}

impl NumUtil {
    pub(crate) fn init() -> Self {
        let nums = "零一二三四五六七八九".to_string();
        let nums_characters: Vec<char> = nums.chars().collect();
        let mut DIGITS: HashMap<String, char> = HashMap::new();
        let mut UNITS: HashMap<usize, char> = HashMap::new();
        for i in 0..nums_characters.len() {
            let key = i.to_string();
            let value = nums_characters[i];
            DIGITS.insert(key, value);
        }
        UNITS.insert(1, '十');
        UNITS.insert(2, '百');
        UNITS.insert(3, '千');
        UNITS.insert(4, '万');
        UNITS.insert(8, '亿');

        let COM_QUANTIFIERS = "(封|艘|把|目|套|段|人|所|朵|匹|张|座|回|场|尾|条|个|首|阙|阵|网|炮|顶|丘|棵|只|支|袭|辆|挑|担|颗|壳|窠|曲|墙|群|腔|砣|座|客|贯|扎|捆|刀|令|打|手|罗|坡|山|岭|江|溪|钟|队|单|双|对|出|口|头|脚|板|跳|枝|件|贴|针|线|管|名|位|身|堂|课|本|页|家|户|层|丝|毫|厘|分|钱|两|斤|担|铢|石|钧|锱|忽|(千|毫|微)克|毫|厘|(公)分|分|寸|尺|丈|里|寻|常|铺|程|(千|分|厘|毫|微)米|米|撮|勺|合|升|斗|石|盘|碗|碟|叠|桶|笼|盆|盒|杯|钟|斛|锅|簋|篮|盘|桶|罐|瓶|壶|卮|盏|箩|箱|煲|啖|袋|钵|年|月|日|季|刻|时|周|天|秒|分|小时|旬|纪|岁|世|更|夜|春|夏|秋|冬|代|伏|辈|丸|泡|粒|颗|幢|堆|条|根|支|道|面|片|张|颗|块|元|(亿|千万|百万|万|千|百)|(亿|千万|百万|万|千|百|美|)元|(亿|千万|百万|万|千|百|十|)吨|(亿|千万|百万|万|千|百|)块|角|毛|分)";

        let RE_FRAC = Regex::new(r"(-?)(\d+)/(\d+)").unwrap();
        let RE_PERCENTAGE = Regex::new(r"(-?)(\d+(\.\d+)?)%").unwrap();
        let RE_RANGE = Regex::new(r"((-?)((\d+)(\.\d+)?)|(\.(\d+)))[-~]((-?)((\d+)(\.\d+)?)|(\.(\d+)))").unwrap();
        let RE_NUMBER = Regex::new(r"(-?)((\d+)(\.\d+)?)|(\.(\d+))").unwrap();
        let RE_INTEGER = Regex::new(r"(-)(\d+)").unwrap();
        // let RE_DECIMAL_NUM = Regex::new(r"^[-+]?\d+(\.\d+)?([eE][-+]?\d+)?$").unwrap();
        let RE_DEFAULT_NUM = Regex::new(r"\d{7}\d*").unwrap();

        let RE_DECIMAL_NUM = Regex::new(r"(-?)((\d+)(\.\d+))|(\.(\d+))").unwrap();

        let mut rtext = r"(\d+)([多余几\+])?".to_string();
        rtext.push_str(COM_QUANTIFIERS);

        let RE_POSITIVE_QUANTIFIERS = Regex::new(&rtext).unwrap();

        NumUtil { DIGITS, UNITS, RE_FRAC, RE_PERCENTAGE, RE_RANGE, RE_NUMBER, RE_INTEGER, RE_DECIMAL_NUM, RE_DEFAULT_NUM, RE_POSITIVE_QUANTIFIERS }
    }


    pub fn replace_number(&self, value_string: String, re: Option<&Regex>) -> String {
        if value_string == "" {
            return value_string;
        }
        let replacement = |caps: &Captures| -> String{
            let sign = caps.get(1).map_or("", |m| m.as_str());
            let number = caps.get(2).map_or(None, |m| Some(m.as_str()));
            let pure_decimal = caps.get(5).map_or(None, |m| Some(m.as_str()));

            let mut result = "".to_string();
            if pure_decimal.is_some() {
                result = self.num2str(pure_decimal.unwrap().to_string());
            } else {
                let sign = {
                    if sign != "" {
                        "负"
                    } else {
                        ""
                    }
                };
                let number = {
                    if number.is_some() {
                        number.unwrap()
                    } else {
                        ""
                    }
                };
                let number = self.num2str(number.to_string());
                result = format!("{}{}", sign, number);
            }

            result
        };


        let caps = {
            if re.is_some(){
                re.unwrap().replace_all(&value_string, replacement).to_string()
            }else {
                self.RE_NUMBER.replace_all(&value_string, replacement).to_string()
            }
        };
        caps
    }


    pub fn replace_frac(&self, value_string: String) -> String {
        let replacement = |caps: &Captures| -> String{
            let sign = caps.get(1).map_or("", |m| m.as_str());
            let nominator = caps.get(2).map_or("", |m| m.as_str());
            let denominator = caps.get(3).map_or("", |m| m.as_str());

            let sign = {
                if sign != "" {
                    "负"
                } else {
                    ""
                }
            };
            let nominator = self.num2str(nominator.to_string());
            let denominator = self.num2str(denominator.to_string());

            let result = format!("{}{}分之{}", sign, denominator, nominator);

            result
        };

        let caps = self.RE_FRAC.replace_all(&value_string, replacement).to_string();
        caps
    }

    pub fn replace_percentage(&self, value_string: String) -> String {
        let replacement = |caps: &Captures| -> String{
            let sign = caps.get(1).map_or("", |m| m.as_str());
            let percent = caps.get(2).map_or("", |m| m.as_str());

            let percent = self.num2str(percent.to_string());
            let sign = {
                if sign != "" {
                    "负"
                } else {
                    ""
                }
            };
            let result = format!("{}百分之{}", sign, percent);
            result
        };

        let caps = self.RE_PERCENTAGE.replace_all(&value_string, replacement).to_string();
        caps
    }

    // test ok
    pub fn replace_negative_num(&self, value_string: String) -> String {
        let replacement = |caps: &Captures| -> String{
            let sign = caps.get(1).map_or("", |m| m.as_str());
            let number = caps.get(2).map_or("", |m| m.as_str());

            let number = self.num2str(number.to_string());

            let sign = {
                if sign != "" {
                    "负"
                } else {
                    ""
                }
            };
            let result = format!("{}{}", sign, number);
            result
        };

        let caps = self.RE_INTEGER.replace_all(&value_string, replacement).to_string();

        caps
    }


    pub fn replace_positive_quantifier(&self, value_string: String) -> String {
        let replacement = |caps: &Captures| -> String{
            let mut result = "".to_string();
            let number: Option<&str> = caps.get(1).map_or(None, |m| Some(m.as_str()));
            let match_2: Option<&str> = caps.get(2).map_or(None, |m| Some(m.as_str()));
            let quantifiers: Option<&str> = caps.get(3).map_or(None, |m| Some(m.as_str()));

            let match_2 = {
                if match_2.is_some() && match_2.unwrap() == "+" {
                    "多"
                } else {
                    ""
                }
            };
            let number = {
                if number.is_some() {
                    number.unwrap()
                } else {
                    ""
                }
            };
            let quantifiers = {
                if quantifiers.is_some() {
                    quantifiers.unwrap()
                } else {
                    ""
                }
            };
            let number = self.num2str(number.to_string());

            result = format!("{}{}{}", number, match_2, quantifiers);
            result
        };

        let caps = self.RE_POSITIVE_QUANTIFIERS.replace_all(&value_string, replacement).to_string();

        caps
    }

    // @test
    pub fn replace_default_num(&self, value_string: String) -> String {
        let replacement = |caps: &Captures| -> String{
            let number = caps.get(0).map_or("", |m| m.as_str());
            if number == "" {
                return "".to_string();
            }
            let result = self.verbalize_digit(number.to_string(), true);
            result
        };

        let caps = self.RE_DEFAULT_NUM.replace_all(&value_string, replacement).to_string();
        caps
    }

    pub fn num2str(&self, value_string: String) -> String {
        if value_string == "" {
            return value_string;
        }
        let mut result = "".to_string();
        let integer_decimal: Vec<&str> = value_string.split('.').collect();
        let mut integer = "";
        let mut decimal = "";
        if integer_decimal.len() == 1 {
            integer = integer_decimal[0];
            decimal = "";
        } else if integer_decimal.len() == 2 {
            integer = integer_decimal[0];
            decimal = integer_decimal[1];
        }

        result = self.verbalize_cardinal(integer.to_string());
        decimal = decimal.trim_end_matches("0");
        if decimal != "" {
            result = {
                if !result.is_empty() {
                    result
                } else {
                    "零".to_string()
                }
            };
            result = result + "点";
            result = result + &self.verbalize_digit(decimal.to_string(), false);
        }
        result
    }

    pub fn replace_range(&self, value_string: String) -> String {
        let replacement = |caps: &Captures| -> String{
            let first = caps.get(1).map_or("", |m| m.as_str());
            let second = caps.get(8).map_or("", |m| m.as_str());


            let first = self.replace_number(first.to_string(), Some(&self.RE_NUMBER));
            let second = self.replace_number(second.to_string(), Some(&self.RE_NUMBER));

            let result = format!("{}到{}", first, second);
            result
        };

        let caps = self.RE_RANGE.replace_all(&value_string, replacement).to_string();
        caps
    }

    fn _get_value(&self, value_string: &str, use_zero: bool) -> Vec<String> {
        let stripped = value_string.trim_start_matches('0');
        if stripped.chars().count() == 0 {
            let outs: Vec<String> = Vec::new();
            return outs;
        } else if stripped.chars().count() == 1 {
            if use_zero && stripped.chars().count() < value_string.chars().count() {
                return vec![self.DIGITS.get("0").unwrap().to_string(), self.DIGITS.get(stripped).unwrap().to_string()];
            } else {
                return vec![self.DIGITS.get(stripped).unwrap().to_string()];
            }
        } else {
            let mut largest_unit: usize = 8;
            let mut keys: Vec<usize> = vec![8, 4, 3, 2, 1];
            for power in keys {
                if power < stripped.chars().count() {
                    largest_unit = power;
                    break;
                }
            }

            let first_part = &value_string[..value_string.chars().count() - largest_unit];
            let second_part = &value_string[value_string.chars().count() - largest_unit..];

            let mut l = self._get_value(first_part, true);
            let m = vec![self.UNITS.get(&largest_unit).unwrap().to_string()];
            let r = self._get_value(second_part, true);
            l.extend(m);
            l.extend(r);
            return l;
        }
    }

    pub fn verbalize_cardinal(&self, sentence: String) -> String {
        let value_string = sentence.trim_start_matches('0');
        if value_string.chars().count() == 0 {
            let d = self.DIGITS.get("0").unwrap().to_string();
            return d;
        }

        let mut result_symbols = self._get_value(value_string, true);
        let d1 = self.DIGITS.get("1").unwrap().to_string();
        let u1 = self.UNITS.get(&1).unwrap().to_string();
        if result_symbols.len() >= 2 && result_symbols[0] == d1 && result_symbols[1] == u1 {
            result_symbols = result_symbols[1..].to_vec();
        }

        let value_string = result_symbols.join("");
        value_string
    }

    pub fn verbalize_digit(&self, sentence: String, alt_one: bool) -> String {
        let sentence_chars: Vec<char> = sentence.chars().collect();

        let mut new_sentence_chars: Vec<char> = Vec::new();
        for ci in sentence_chars {
            let key = ci.to_string();
            if self.DIGITS.contains_key(&key) {
                let si = self.DIGITS.get(&key).unwrap().clone();
                new_sentence_chars.push(si);
            } else {
                new_sentence_chars.push(ci);
            }
        }

        let mut new_sentence = String::from_iter(new_sentence_chars);

        if alt_one {
            new_sentence = new_sentence.replace("一", "幺");
        }
        new_sentence
    }
}