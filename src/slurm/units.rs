//! Slurm's time and memory notations, in both directions.

/// Slurm time strings: `MM`, `MM:SS`, `HH:MM:SS`, `D-HH`, `D-HH:MM:SS`.
pub fn parse_time(text: &str) -> Option<u64> {
    let text = text.trim();
    if text.is_empty()
        || text.eq_ignore_ascii_case("UNLIMITED")
        || text.eq_ignore_ascii_case("infinite")
        || text.eq_ignore_ascii_case("NONE")
    {
        return None;
    }

    let (days, rest) = match text.split_once('-') {
        Some((days, rest)) => (days.parse::<u64>().ok()?, rest),
        None => (0, text),
    };

    let parts: Vec<u64> = rest
        .split(':')
        .map(|p| p.parse::<u64>().ok())
        .collect::<Option<_>>()?;

    let minutes = match parts.as_slice() {
        // A bare number is minutes, unless days were given, when it is hours.
        [only] if days > 0 => only * 60,
        [only] => *only,
        [h, m] if days > 0 => h * 60 + m,
        [m, s] => m + u64::from(*s >= 30),
        [h, m, s] => h * 60 + m + u64::from(*s >= 30),
        _ => return None,
    };
    Some(days * 24 * 60 + minutes)
}

pub fn format_time(minutes: u64) -> String {
    let (days, rest) = (minutes / (24 * 60), minutes % (24 * 60));
    let (hours, mins) = (rest / 60, rest % 60);
    if days > 0 {
        format!("{days}-{hours:02}:{mins:02}:00")
    } else {
        format!("{hours:02}:{mins:02}:00")
    }
}

/// Slurm memory strings: a number, optionally suffixed K, M, G or T.
pub fn parse_mem(text: &str) -> Option<u64> {
    let text = text.trim();
    if text.is_empty() || text.eq_ignore_ascii_case("UNLIMITED") || text == "0" {
        return None;
    }
    // A bare number is megabytes, which is how Slurm reports node memory.
    let split = text
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len());
    let (digits, suffix) = text.split_at(split);
    let digits: u64 = digits.parse().ok()?;
    Some(match suffix.trim().to_ascii_uppercase().as_str() {
        "" | "M" | "MB" => digits,
        "K" | "KB" => digits / 1024,
        "G" | "GB" => digits * 1024,
        "T" | "TB" => digits * 1024 * 1024,
        _ => return None,
    })
}

pub fn format_mem(mb: u64) -> String {
    if mb < 1024 {
        return format!("{mb}M");
    }
    let gb = mb as f64 / 1024.0;
    if (gb.round() - gb).abs() < 0.05 {
        format!("{}G", gb.round() as u64)
    } else {
        format!("{gb:.1}G")
    }
}
