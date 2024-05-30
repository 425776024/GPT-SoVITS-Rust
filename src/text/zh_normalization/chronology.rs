use std::ops::Add;
// use regex::{Captures,Regex};
use fancy_regex::{Captures, Regex};
use crate::tts_sovits::text::zh_normalization::num::NumUtil;

pub struct Chronology {
    pub RE_DATE: Regex,
    pub RE_DATE2: Regex,
    pub RE_TIME_RANGE: Regex,
    pub RE_TIME: Regex,
    pub num_util: NumUtil,
}

impl Chronology {
    pub fn init() -> Self {
        let RE_DATE = Regex::new(r"(\d{4}|\d{2})年((0?[1-9]|1[0-2])月)?(((0?[1-9])|((1|2)[0-9])|30|31)([日号]))?").unwrap();
        let RE_DATE2 = Regex::new(r"(\d{4}|\d{2})[- /.](0[1-9]|1[0-2]|[1-9])[- /.](0[1-9]|1[0-9]|2[0-9]|[1-9]|30|31)([日号])?").unwrap();

        // 时间范围，如8:30-12:30
        let RE_TIME_RANGE = Regex::new(r"([0-1]?[0-9]|2[0-3]):([0-5][0-9])(:([0-5][0-9]))?(~|-)([0-1]?[0-9]|2[0-3]):([0-5][0-9])(:([0-5][0-9]))?").unwrap();

        // 时刻表达式
        let RE_TIME = Regex::new(r"([0-1]?[0-9]|2[0-3]):([0-5][0-9])(:([0-5][0-9]))?").unwrap();

        let num_util = NumUtil::init();

        Chronology {
            RE_DATE,
            RE_DATE2,
            RE_TIME_RANGE,
            RE_TIME,
            num_util,
        }
    }

    pub fn _time_num2str(&self, num_string: String) -> String {
        let t = num_string.trim_start_matches("0").to_string();
        let mut result = self.num_util.num2str(t);
        if num_string.starts_with("0") {
            result = self.num_util.DIGITS.get("0").unwrap().to_string().add(&result);
        }
        result
    }

    /// 日期转化
    pub fn replace_date(&self, sentence: String) -> String {
        let replacement = |caps: &Captures| -> String{
            let mut result = "".to_string();

            let year: Option<&str> = caps.get(1).map_or(None, |m| Some(m.as_str()));
            let month: Option<&str> = caps.get(3).map_or(None, |m| Some(m.as_str()));
            let day: Option<&str> = caps.get(5).map_or(None, |m| Some(m.as_str()));
            let group: Option<&str> = caps.get(9).map_or(None, |m| Some(m.as_str()));

            if year.is_some() {
                let year = year.unwrap().to_string();
                let vd = self.num_util.verbalize_digit(year, false);
                result = result.add(&format!("{}年", vd));
            }
            if month.is_some() {
                let month = month.unwrap().to_string();
                let vd = self.num_util.verbalize_cardinal(month);
                result = result.add(&format!("{}月", vd));
            }
            if day.is_some() {
                let day = day.unwrap().to_string();
                let group = group.unwrap().to_string();
                let vd = self.num_util.verbalize_cardinal(day);
                result = result.add(&format!("{}{}", vd, group));
            }
            result
        };

        let caps = self.RE_DATE.replace_all(&sentence, replacement).to_string();
        caps
    }

    pub fn replace_date2(&self, sentence: String) -> String {
        let replacement = |caps: &Captures| -> String{
            let mut result = "".to_string();

            let year: Option<&str> = caps.get(1).map_or(None, |m| Some(m.as_str()));
            let month: Option<&str> = caps.get(2).map_or(None, |m| Some(m.as_str()));
            let day: Option<&str> = caps.get(3).map_or(None, |m| Some(m.as_str()));
            // 日后面有日
            let day_back_day: Option<&str> = caps.get(4).map_or(None, |m| Some(m.as_str()));

            if year.is_some() {
                let year = year.unwrap().to_string();
                let vd = self.num_util.verbalize_digit(year, false);
                result = result.add(&format!("{}年", vd));
            }
            if month.is_some() {
                let month = month.unwrap().to_string();
                let vd = self.num_util.verbalize_cardinal(month);
                result = result.add(&format!("{}月", vd));
            }
            if day.is_some() {
                let day = day.unwrap().to_string();
                let vd = self.num_util.verbalize_cardinal(day);
                if day_back_day.is_none() {
                    result = result.add(&format!("{}日", vd));
                } else {
                    result = result.add(&format!("{}{}", vd, day_back_day.unwrap()));
                }
            }
            result
        };

        let caps = self.RE_DATE2.replace_all(&sentence, replacement).to_string();
        caps
    }

    pub fn replace_time(&self, sentence: String, regex: &Regex) -> String {
        let replacement = |caps: &Captures| -> String{
            let mut result = "".to_string();

            let is_range = caps.len() > 5;

            let hour: Option<&str> = caps.get(1).map_or(None, |m| Some(m.as_str()));
            let minute: Option<&str> = caps.get(2).map_or(None, |m| Some(m.as_str()));
            let second: Option<&str> = caps.get(4).map_or(None, |m| Some(m.as_str()));

            if hour.is_some() && minute.is_some() {
                let hour = hour.unwrap().to_string();
                let minute = minute.unwrap().to_string();
                result = format!("{}点", self.num_util.num2str(hour));
                if minute.trim_start_matches("0").is_empty() == false {
                    let i_minute: i32 = minute.parse().unwrap();
                    if i_minute == 30 {
                        result = result.add("半");
                    } else {
                        result = format!("{}{}分", result, self._time_num2str(minute))
                    }
                }
            }
            if second.is_some() {
                let second = second.unwrap().to_string();
                if second.trim_start_matches("0").is_empty() == false {
                    result = format!("{}{}秒", result, self._time_num2str(second))
                }
            }

            if is_range {
                let hour2: Option<&str> = caps.get(6).map_or(None, |m| Some(m.as_str()));
                let minute2: Option<&str> = caps.get(7).map_or(None, |m| Some(m.as_str()));
                let second2: Option<&str> = caps.get(9).map_or(None, |m| Some(m.as_str()));

                result += "至";
                if hour2.is_some() {
                    let hour2 = hour2.unwrap().to_string();
                    result = format!("{}{}点", result, self.num_util.num2str(hour2))
                }
                if minute2.is_some() {
                    let minute2 = minute2.unwrap().to_string();
                    if minute2.trim_start_matches("0").is_empty() == false {
                        let i_minute: i32 = minute2.parse().unwrap();
                        if i_minute == 30 {
                            result = result.add("半");
                        } else {
                            result = format!("{}{}分", result, self._time_num2str(minute2))
                        }
                    }
                    if second2.is_some() {
                        let second2 = second2.unwrap().to_string();
                        if second2.trim_start_matches("0").is_empty() == false {
                            result = result.add(&format!("{}秒", self._time_num2str(second2)));
                        }
                    }
                }
            }

            result
        };

        let caps = regex.replace_all(&sentence, replacement).to_string();
        caps
    }
}