const SECONDS_PER_DAY: i64 = 86_400;

pub fn escape_inline_code(input: &str) -> String {
    sanitize_text(input).replace('`', "\\`")
}

pub fn escape_markdown(input: &str) -> String {
    let mut output = String::new();
    for character in sanitize_text(input).chars() {
        if matches!(character, '\\' | '*' | '_' | '[' | ']') {
            output.push('\\');
        }
        output.push(character);
    }
    output
}

pub fn sanitize_text(input: &str) -> String {
    input
        .chars()
        .map(|character| if character.is_control() { '�' } else { character })
        .collect()
}

pub fn matched_keywords(subject: &str, keywords: &[String], substring: bool) -> Vec<String> {
    let subject = subject.to_lowercase();
    let mut matched = keywords
        .iter()
        .filter_map(|keyword| {
            let keyword = keyword.trim().to_lowercase();
            if keyword.is_empty() {
                return None;
            }
            let matches = if substring {
                subject.contains(&keyword)
            } else {
                subject
                    .match_indices(&keyword)
                    .any(|(start, _)| is_word_match(&subject, start, keyword.len()))
            };
            matches.then_some(keyword)
        })
        .collect::<Vec<_>>();
    matched.sort();
    matched.dedup();
    matched
}

fn is_word_match(subject: &str, start: usize, length: usize) -> bool {
    let before = subject[..start].chars().next_back();
    let after = subject[start + length..].chars().next();
    !before.is_some_and(is_word_character) && !after.is_some_and(is_word_character)
}

fn is_word_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

pub fn in_window(timestamp: i64, now: i64, days: u32) -> bool {
    timestamp >= now.saturating_sub(i64::from(days).saturating_mul(SECONDS_PER_DAY))
}

pub fn month_for_timestamp(seconds: i64) -> String {
    let days = seconds.div_euclid(SECONDS_PER_DAY);
    let (year, month, _) = civil_date_from_days(days);
    format!("{year:04}-{month:02}")
}

/// Reports use a UTC capture date as their stable reference marker. Analysis
/// windows still use the precise process clock internally, while repeated
/// runs of unchanged inputs remain byte-comparable within one UTC day.
pub fn capture_date(time: std::time::SystemTime) -> String {
    let seconds = time
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    timestamp_to_rfc3339_seconds(seconds - seconds.rem_euclid(SECONDS_PER_DAY))
}

pub fn timestamp_to_rfc3339_seconds(seconds: i64) -> String {
    let days = seconds.div_euclid(SECONDS_PER_DAY);
    let seconds_of_day = seconds.rem_euclid(SECONDS_PER_DAY);
    let (year, month, day) = civil_date_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = seconds_of_day.rem_euclid(3_600) / 60;
    let second = seconds_of_day.rem_euclid(60);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Gregorian calendar conversion based on the civil-from-days algorithm.
pub fn civil_date_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_part + 2) / 5 + 1;
    let month = month_part + if month_part < 10 { 3 } else { -9 };
    let year = year + i64::from(month <= 2);
    (year, month, day)
}

pub fn token_count(text: &str) -> usize {
    text.chars().count().div_ceil(4).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_date_conversion_handles_epoch_and_month_boundaries() {
        assert_eq!(civil_date_from_days(0), (1970, 1, 1));
        assert_eq!(civil_date_from_days(18_262), (2020, 1, 1));
        assert_eq!(month_for_timestamp(-1), "1969-12");
    }

    #[test]
    fn keyword_matching_is_case_insensitive_word_aware_and_supports_substrings() {
        let keywords = vec!["Fix".to_owned(), "bug".to_owned()];
        assert_eq!(matched_keywords("FIX parser", &keywords, false), ["fix"]);
        assert_eq!(matched_keywords("a BUG report", &keywords, false), ["bug"]);
        assert!(matched_keywords("fixture prefix debug", &keywords, false).is_empty());
        assert_eq!(matched_keywords("fixture", &keywords, true), ["fix"]);
        assert!(matched_keywords("feature", &[String::new()], false).is_empty());
    }
}
