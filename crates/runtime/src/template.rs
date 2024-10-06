
pub fn seeded_rng(seed: &str) -> impl rand::Rng {
    rand_seeder::Seeder::from(seed).make_rng::<rand_pcg::Pcg64>()
}

pub fn default_offset() -> time::UtcOffset {
    // TODO: use tz-rs or something to get the right timezone?
    time::UtcOffset::UTC
}

fn short_month(month: time::Month) -> &'static str {
    match month {
        time::Month::January => "Jan",
        time::Month::February => "Feb",
        time::Month::March => "Mar",
        time::Month::April => "Apr",
        time::Month::May => "May",
        time::Month::June => "Jun",
        time::Month::July => "Jul",
        time::Month::August => "Aug",
        time::Month::September => "Sep",
        time::Month::October => "Oct",
        time::Month::November => "Nov",
        time::Month::December => "Dec",
    }
}

#[derive(Default)]
pub enum DateFmt {
    #[default]
    Short,
    Shorter,
}

pub fn format_date(date: &time::OffsetDateTime, offset: Option<time::UtcOffset>, mode: DateFmt) -> String {
    let offset = offset.unwrap_or_else(default_offset);
    let current = time::OffsetDateTime::now_utc().to_offset(offset);
    let current_year = current.year();
    let date = date.to_offset(offset);

    let year = date.year();
    let month = short_month(date.month());
    let day = date.day();
    let hour = date.hour();
    let minute = date.minute();

    match mode {
        DateFmt::Short => {
            if year == current_year {
                format!("{month} {day:02}, {hour:02}:{minute:02}")
            } else {
                format!("{month} {day:02} {year:04}, {hour:02}:{minute:02}")
            }
        },
        DateFmt::Shorter => {
            if year == current_year {
                format!("{month} {day:02}")
            } else {
                format!("{month} {day:02} {year:04}")
            }
        }
    }
}

#[derive(Default)]
pub enum DurationFmt {
    #[default]
    Shorter,
    Short,
    Long,
}

pub fn format_age(date: &time::OffsetDateTime, fmt: DurationFmt) -> String {
    use std::fmt::Write;

    let dur = time::OffsetDateTime::now_utc() - *date;

    // TODO: weeks/months, or just shift back to date after a point
    let weeks = dur.whole_weeks();
    let days = dur.whole_days();
    let hours = dur.whole_hours() % 24;
    let minutes = dur.whole_minutes() % 60;
    let seconds = dur.as_seconds_f64() % 60.0;
    let mut out = String::new();

    match fmt {
        DurationFmt::Shorter => {
            if days > 31 {
                return format_date(date, None, DateFmt::Shorter);
            } else if weeks > 0 {
                write!(out, "{}w", weeks).unwrap();
            } else if days > 0 {
                write!(out, "{}d", days).unwrap();
            } else if hours > 0 {
                write!(out, "{}h", hours).unwrap();
            } else if minutes > 0 {
                write!(out, "{}m", minutes).unwrap();
            } else {
                let seconds = seconds.round() as i64;
                write!(out, "{}s", seconds).unwrap();
            }
            write!(out, " ago").unwrap();
        },
        DurationFmt::Short => {
            if weeks > 0 {
                write!(out, "{} week{}", weeks, plural(weeks)).unwrap();
            } else if days > 0 {
                write!(out, "{} day{}", days, plural(days)).unwrap();
            } else if hours > 0 {
                write!(out, "{} hour{}", hours, plural(hours)).unwrap();
            } else if minutes > 0 {
                write!(out, "{} min{}", minutes, plural(minutes)).unwrap();
            } else {
                let seconds = seconds.round() as i64;
                write!(out, "{} sec{}", seconds, plural(seconds)).unwrap();
            }
            write!(out, " ago").unwrap();
        },
        DurationFmt::Long => {
            if days > 0 { write!(out, "{} day{} ", days, plural(days)).unwrap(); }
            if hours > 0 { write!(out, "{} hour{} ", hours, plural(hours)).unwrap(); }
            if minutes > 0 { write!(out, "{} minute{} ", minutes, plural(minutes)).unwrap(); }
            if days <= 0 && hours <= 0 && (seconds > 0.0 || minutes <= 0) {
                write!(out, "{seconds:.3} seconds ").unwrap();
            }
            write!(out, "ago").unwrap();
        }
    }
    out
}

fn plural(number: i64) -> &'static str {
    if number != 1 { "s" } else { "" }
}
