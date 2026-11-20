use crate::{Result, message};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Money(pub i64);

impl Money {
    pub fn parse(input: &str) -> Option<Self> {
        let text: String = input
            .chars()
            .filter_map(|c| match c {
                '０'..='９' => char::from_u32(c as u32 - '０' as u32 + '0' as u32),
                '．' => Some('.'),
                '，' | ',' | '¥' | '￥' => None,
                c if c.is_whitespace() => None,
                c => Some(c),
            })
            .collect();
        let (negative, text) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text.as_str()),
        };
        let mut parts = text.split('.');
        let whole = parts.next()?;
        let fraction = parts.next().unwrap_or("");
        if whole.is_empty()
            || !whole.bytes().all(|b| b.is_ascii_digit())
            || fraction.len() > 2
            || !fraction.bytes().all(|b| b.is_ascii_digit())
            || parts.next().is_some()
        {
            return None;
        }
        let cents = match fraction.len() {
            0 => 0,
            1 => fraction.parse::<i64>().ok()? * 10,
            _ => fraction.parse::<i64>().ok()?,
        };
        let total = whole
            .parse::<i64>()
            .ok()?
            .checked_mul(100)?
            .checked_add(cents)?;
        Some(Self(if negative { -total } else { total }))
    }

    pub fn filename_value(self) -> String {
        if self.0 % 100 == 0 {
            (self.0 / 100).to_string()
        } else {
            self.to_string()
        }
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let amount = self.0.unsigned_abs();
        write!(
            f,
            "{}{}.{:02}",
            if self.0 < 0 { "-" } else { "" },
            amount / 100,
            amount % 100
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", content = "values", rename_all = "snake_case")]
pub enum Recognition<T> {
    Missing,
    Recognized(T),
    Conflict(Vec<T>),
}

impl<T: Clone + PartialEq> Recognition<T> {
    pub fn from_candidates(values: Vec<T>) -> Self {
        let mut unique = Vec::new();
        for value in values {
            if !unique.contains(&value) {
                unique.push(value);
            }
        }
        match unique.len() {
            0 => Self::Missing,
            1 => Self::Recognized(unique.remove(0)),
            _ => Self::Conflict(unique),
        }
    }

    pub fn value(&self) -> Option<&T> {
        match self {
            Self::Recognized(value) => Some(value),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceData {
    pub page_count: usize,
    pub has_text: bool,
    pub number: Recognition<String>,
    pub date: Recognition<String>,
    pub amount: Recognition<Money>,
    pub seller: Recognition<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Review {
    pub number: String,
    pub amount: String,
    pub confirmed: bool,
}

impl Review {
    pub fn from_invoice(data: &InvoiceData) -> Self {
        Self {
            number: data.number.value().cloned().unwrap_or_default(),
            amount: data
                .amount
                .value()
                .map(ToString::to_string)
                .unwrap_or_default(),
            confirmed: false,
        }
    }

    pub fn validated(&self, data: &InvoiceData) -> Result<(String, Money)> {
        let number = self.number.trim();
        if !(8..=20).contains(&number.len()) || !number.bytes().all(|b| b.is_ascii_digit()) {
            return Err(message("请填写 8～20 位数字的完整发票号码"));
        }
        let amount =
            Money::parse(&self.amount).ok_or_else(|| message("金额须为最多两位小数的数字"))?;
        if (data.number.value().is_none() || data.amount.value().is_none()) && !self.confirmed {
            return Err(message(
                "关键字段未识别或存在冲突，请核对票面、补填后勾选确认",
            ));
        }
        Ok((number.to_owned(), amount))
    }
}

pub fn filename(name: &str, group: &str, amount: Money, number: &str) -> Result<String> {
    crate::config::validate_personal(name, "姓名")?;
    crate::config::validate_personal(group, "组别")?;
    if number.len() < 4 || !number.bytes().all(|b| b.is_ascii_digit()) {
        return Err(message("发票号码无效"));
    }
    Ok(format!(
        "{name}_{group}_{}_{}.pdf",
        amount.filename_value(),
        &number[number.len() - 4..]
    ))
}
