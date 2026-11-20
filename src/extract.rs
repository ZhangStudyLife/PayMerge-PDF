use crate::{
    Result, message,
    model::{InvoiceData, Money, Recognition},
    pdf,
};
use chrono::NaiveDate;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::LazyLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Glyph {
    pub ch: char,
    pub x: f32,
    /// Baseline distance from the top of the page; punctuation has a much smaller ink box.
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageText {
    pub glyphs: Vec<Glyph>,
}

#[derive(Debug)]
struct Line {
    glyphs: Vec<Glyph>,
    y: f32,
    height: f32,
}

fn normalized(c: char) -> char {
    match c {
        '０'..='９' => char::from_u32(c as u32 - '０' as u32 + '0' as u32).unwrap(),
        '：' => ':',
        '（' => '(',
        '）' => ')',
        '￥' => '¥',
        '，' => ',',
        '．' => '.',
        _ => c,
    }
}

fn lines(page: &PageText) -> Vec<Line> {
    let mut glyphs: Vec<_> = page
        .glyphs
        .iter()
        .filter(|g| !g.ch.is_whitespace() && !g.ch.is_control())
        .cloned()
        .collect();
    glyphs.sort_by(|a, b| a.y.total_cmp(&b.y));
    let mut output: Vec<Line> = Vec::new();
    for mut glyph in glyphs {
        glyph.ch = normalized(glyph.ch);
        let center = glyph.y;
        if let Some(line) = output.last_mut()
            && (line.y - center).abs() <= (line.height.min(glyph.height) * 0.6).max(3.0)
        {
            line.glyphs.push(glyph);
        } else {
            output.push(Line {
                y: center,
                height: glyph.height.max(1.0),
                glyphs: vec![glyph],
            });
        }
    }
    for line in &mut output {
        line.glyphs.sort_by(|a, b| a.x.total_cmp(&b.x));
    }
    output
}

impl Line {
    fn anchors(&self, label: &str) -> Vec<usize> {
        let hay: Vec<_> = self.glyphs.iter().map(|g| g.ch).collect();
        let needle: Vec<_> = label.chars().collect();
        hay.windows(needle.len())
            .enumerate()
            .filter_map(|(i, part)| (part == needle).then_some(i))
            .collect()
    }

    fn after(&self, index: usize, count: usize) -> String {
        self.glyphs[index + count..].iter().map(|g| g.ch).collect()
    }

    fn name_after(&self, index: usize, count: usize) -> String {
        let mut value = String::new();
        let mut previous = &self.glyphs[index + count - 1];
        for glyph in &self.glyphs[index + count..] {
            if glyph.x - previous.x - previous.width > previous.height.max(glyph.height) * 2.0 {
                break;
            }
            value.push(glyph.ch);
            previous = glyph;
        }
        value
    }
}

static NUMBER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|[^0-9A-Za-z])([0-9]{8,20})(?:[^0-9A-Za-z]|$)").unwrap());
static DATE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([0-9]{4})[-年/.]([0-9]{1,2})[-月/.]([0-9]{1,2})日?").unwrap());
static AMOUNT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|[^0-9A-Za-z])(-?[0-9][0-9,]*\.[0-9]{2})(?:[^0-9]|$)").unwrap()
});

fn numbers(text: &str) -> Vec<String> {
    NUMBER
        .captures_iter(text)
        .map(|m| m[1].to_owned())
        .collect()
}

fn dates(text: &str) -> Vec<String> {
    DATE.captures_iter(text)
        .filter_map(|m| {
            NaiveDate::from_ymd_opt(m[1].parse().ok()?, m[2].parse().ok()?, m[3].parse().ok()?)
                .map(|date| date.format("%Y-%m-%d").to_string())
        })
        .collect()
}

fn amounts(text: &str) -> Vec<Money> {
    AMOUNT
        .captures_iter(text)
        .filter_map(|m| Money::parse(&m[1]))
        .collect()
}

// Limit every candidate to the labelled row, or its immediately adjacent value row.
fn labelled<T>(rows: &[Line], labels: &[&str], parse: fn(&str) -> Vec<T>) -> Vec<T> {
    let mut values = Vec::new();
    for (row_index, row) in rows.iter().enumerate() {
        for label in labels {
            for start in row.anchors(label) {
                let direct = parse(&row.after(start, label.chars().count()));
                if !direct.is_empty() {
                    values.extend(direct);
                    continue;
                }
                if let Some(next) = rows.get(row_index + 1)
                    && next.y - row.y <= row.height * 2.2
                {
                    let left = row.glyphs[start].x;
                    let text: String = next
                        .glyphs
                        .iter()
                        .filter(|g| g.x >= left - 2.0)
                        .map(|g| g.ch)
                        .collect();
                    values.extend(parse(&text));
                }
            }
        }
    }
    values
}

fn seller_name(text: &str) -> Option<String> {
    let mut value = text.trim_start_matches([':', '(', ')']).to_owned();
    for stop in [
        "统一社会",
        "纳税人",
        "识别号",
        "税号",
        "地址",
        "电话",
        "名称:",
    ] {
        if let Some(index) = value.find(stop) {
            value.truncate(index);
        }
    }
    (value.chars().count() >= 4
        && value
            .chars()
            .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)))
    .then_some(value)
}

fn sellers(rows: &[Line]) -> Vec<String> {
    let mut output = Vec::new();
    for row in rows {
        for index in row.anchors("销售方名称") {
            if let Some(value) = seller_name(&row.name_after(index, 5)) {
                output.push(value);
            }
        }
    }
    if !output.is_empty() {
        return output;
    }
    let mut anchors = Vec::new();
    for row in rows {
        for start in row.anchors("销售方") {
            anchors.push((row.glyphs[start].x, row.y));
        }
        // Vertical “销售方信息” labels in current electronic invoice templates.
        for glyph in row.glyphs.iter().filter(|g| g.ch == '销') {
            if rows.iter().any(|r| {
                r.y > row.y
                    && r.y - row.y < glyph.height * 3.5
                    && r.glyphs
                        .iter()
                        .any(|g| g.ch == '售' && (g.x - glyph.x).abs() < glyph.height)
            }) {
                anchors.push((glyph.x, row.y));
            }
        }
    }
    for (x, y) in anchors {
        let nearest = rows
            .iter()
            .flat_map(|row| row.anchors("名称").into_iter().map(move |i| (row, i)))
            .filter(|(row, i)| row.glyphs[*i].x >= x - 2.0 && (row.y - y).abs() < row.height * 8.0)
            .min_by(|(a, ia), (b, ib)| {
                ((a.y - y).abs() + (a.glyphs[*ia].x - x).abs() * 0.1)
                    .total_cmp(&((b.y - y).abs() + (b.glyphs[*ib].x - x).abs() * 0.1))
            });
        if let Some((row, index)) = nearest
            && let Some(value) = seller_name(&row.name_after(index, 2))
        {
            output.push(value);
        }
    }
    output
}

pub fn from_pages(pages: &[PageText]) -> InvoiceData {
    let mut number = Vec::new();
    let mut date = Vec::new();
    let mut amount = Vec::new();
    let mut seller = Vec::new();
    for page in pages {
        let rows = lines(page);
        number.extend(labelled(&rows, &["发票号码", "发票号"], numbers));
        date.extend(labelled(&rows, &["开票日期"], dates));
        let precise = labelled(&rows, &["小写"], amounts);
        amount.extend(if precise.is_empty() {
            labelled(
                &rows,
                &["价税合计", "总金额", "合计金额", "应付金额"],
                amounts,
            )
        } else {
            precise
        });
        seller.extend(sellers(&rows));
    }
    InvoiceData {
        page_count: pages.len(),
        has_text: pages
            .iter()
            .any(|p| p.glyphs.iter().any(|g| !g.ch.is_whitespace())),
        number: Recognition::from_candidates(number),
        date: Recognition::from_candidates(date),
        amount: Recognition::from_candidates(amount),
        seller: Recognition::from_candidates(seller),
    }
}

pub fn read_pages(path: &Path) -> Result<Vec<PageText>> {
    let document = pdf::engine()?.load_pdf_from_file(path, None)?;
    if document.pages().is_empty() {
        return Err(message("PDF 没有页面"));
    }
    let mut pages = Vec::new();
    for page in document.pages().iter() {
        let text = page.text()?;
        let mut glyphs = Vec::new();
        for character in text.chars().iter() {
            if let Some(ch) = character.unicode_char()
                && !ch.is_whitespace()
                && !ch.is_control()
            {
                let bounds = character.tight_bounds()?;
                glyphs.push(Glyph {
                    ch,
                    x: bounds.left().value,
                    y: page.height().value - character.origin_y()?.value,
                    width: bounds.width().value,
                    height: bounds.height().value,
                });
            }
        }
        pages.push(PageText { glyphs });
    }
    Ok(pages)
}

pub fn extract_invoice(path: &Path) -> Result<InvoiceData> {
    Ok(from_pages(&read_pages(path)?))
}
