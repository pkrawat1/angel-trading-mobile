//! Number formatting and small helpers shared by the views.

use dioxus::prelude::ServerFnError;

/// Indian digit grouping (12,34,567.89), the way the broker shows amounts.
pub fn grouped(value: f64, decimals: usize) -> String {
    let formatted = format!("{:.*}", decimals, value.abs());
    let (int_part, frac) = match formatted.split_once('.') {
        Some((int_part, frac)) => (int_part, Some(frac)),
        None => (formatted.as_str(), None),
    };
    let mut out = String::new();
    if value < 0.0 && formatted.chars().any(|c| c.is_ascii_digit() && c != '0') {
        out.push('-');
    }
    out.push_str(&group_indian(int_part));
    if let Some(frac) = frac {
        out.push('.');
        out.push_str(frac);
    }
    out
}

fn group_indian(digits: &str) -> String {
    if digits.len() <= 3 {
        return digits.to_string();
    }
    let (head, tail) = digits.split_at(digits.len() - 3);
    let mut groups = Vec::new();
    let mut end = head.len();
    while end > 2 {
        groups.push(&head[end - 2..end]);
        end -= 2;
    }
    groups.push(&head[..end]);
    groups.reverse();
    format!("{},{tail}", groups.join(","))
}

/// `₹1,23,456.78`, with a leading minus for negatives.
pub fn inr(value: f64) -> String {
    let body = grouped(value.abs(), 2);
    if value < 0.0 && body != "0.00" {
        format!("-₹{body}")
    } else {
        format!("₹{body}")
    }
}

/// Always signed, for P&L: `+₹1,234.00`, `-₹56.70`.
pub fn signed_inr(value: f64) -> String {
    let body = grouped(value.abs(), 2);
    if body == "0.00" {
        format!("₹{body}")
    } else if value > 0.0 {
        format!("+₹{body}")
    } else {
        format!("-₹{body}")
    }
}

/// A price without the currency symbol: `1,234.50`.
pub fn price(value: f64) -> String {
    grouped(value, 2)
}

/// `+1.23%`, `-0.45%`, `0.00%`.
pub fn signed_pct(value: f64) -> String {
    let body = format!("{:.2}", value.abs());
    if body == "0.00" {
        "0.00%".to_string()
    } else if value > 0.0 {
        format!("+{body}%")
    } else {
        format!("-{body}%")
    }
}

/// Short form for volumes and large quantities: `950`, `12.30K`, `4.56L`, `7.89Cr`.
pub fn compact(value: f64) -> String {
    let magnitude = value.abs();
    let (scaled, suffix) = if magnitude >= 1e7 {
        (value / 1e7, "Cr")
    } else if magnitude >= 1e5 {
        (value / 1e5, "L")
    } else if magnitude >= 1e3 {
        (value / 1e3, "K")
    } else {
        return format!("{value:.0}");
    };
    format!("{scaled:.2}{suffix}")
}

/// Drops the `-EQ` series that most NSE equities carry. Other series stay,
/// because SBIN-BE is a different instrument from SBIN-EQ, and a hyphen can be
/// part of the name, as in BAJAJ-AUTO-EQ.
pub fn display_symbol(trading_symbol: &str) -> &str {
    trading_symbol.strip_suffix("-EQ").unwrap_or(trading_symbol)
}

/// DaisyUI text colour for a gain, a loss or a flat value.
pub fn tone(value: f64) -> &'static str {
    if value > 0.0 {
        "text-success"
    } else if value < 0.0 {
        "text-error"
    } else {
        "text-base-content/70"
    }
}

/// The message to show for a failed server call.
pub fn error_text(err: &ServerFnError) -> String {
    match err {
        ServerFnError::ServerError(message) => message.clone(),
        ServerFnError::Request(_) => {
            "Can't reach the trading server. Check that it is running.".to_string()
        }
        other => other.to_string(),
    }
}

/// SmartAPI codes for a missing, invalid or expired session.
const SESSION_ERROR_CODES: [&str; 7] = [
    "AG8001", "AG8002", "AG8003", "AB8050", "AB8051", "AB1010", "AB1011",
];

pub fn is_session_error(err: &ServerFnError) -> bool {
    let text = err.to_string();
    SESSION_ERROR_CODES.iter().any(|code| text.contains(code)) || text.contains("Invalid Token")
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn sleep_ms(ms: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
}

#[cfg(target_arch = "wasm32")]
pub async fn sleep_ms(ms: u64) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        if let Some(window) = web_sys::window() {
            let _ = window
                .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms as i32);
        }
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indian_digit_grouping() {
        assert_eq!(grouped(0.0, 2), "0.00");
        assert_eq!(grouped(999.5, 2), "999.50");
        assert_eq!(grouped(1234.5, 2), "1,234.50");
        assert_eq!(grouped(123456.789, 2), "1,23,456.79");
        assert_eq!(grouped(12345678.0, 0), "1,23,45,678");
        assert_eq!(grouped(-98765.4, 1), "-98,765.4");
        assert_eq!(grouped(-0.001, 2), "0.00", "no negative zero");
    }

    #[test]
    fn rupee_amounts() {
        assert_eq!(inr(1234.5), "₹1,234.50");
        assert_eq!(inr(-56.7), "-₹56.70");
        assert_eq!(inr(-0.001), "₹0.00");
        assert_eq!(signed_inr(1234.0), "+₹1,234.00");
        assert_eq!(signed_inr(-56.7), "-₹56.70");
        assert_eq!(signed_inr(0.004), "₹0.00");
        assert_eq!(price(3388.9), "3,388.90");
    }

    #[test]
    fn percentages() {
        assert_eq!(signed_pct(1.234), "+1.23%");
        assert_eq!(signed_pct(-0.456), "-0.46%");
        assert_eq!(signed_pct(-0.001), "0.00%");
    }

    #[test]
    fn compact_quantities() {
        assert_eq!(compact(950.0), "950");
        assert_eq!(compact(12_300.0), "12.30K");
        assert_eq!(compact(456_000.0), "4.56L");
        assert_eq!(compact(78_900_000.0), "7.89Cr");
        assert_eq!(compact(-1_500.0), "-1.50K");
    }

    #[test]
    fn symbols_lose_only_the_eq_series() {
        assert_eq!(display_symbol("SBIN-EQ"), "SBIN");
        assert_eq!(display_symbol("SBIN-BE"), "SBIN-BE");
        assert_eq!(display_symbol("BAJAJ-AUTO-EQ"), "BAJAJ-AUTO");
        assert_eq!(display_symbol("NIFTY28OCT2625200CE"), "NIFTY28OCT2625200CE");
        assert_eq!(display_symbol("Nifty 50"), "Nifty 50");
    }

    #[test]
    fn tones() {
        assert_eq!(tone(0.5), "text-success");
        assert_eq!(tone(-0.5), "text-error");
        assert_eq!(tone(0.0), "text-base-content/70");
    }

    #[test]
    fn session_errors() {
        let err = |message: &str| ServerFnError::ServerError(message.to_string());
        assert!(is_session_error(&err("Invalid Token [AG8001]")));
        assert!(is_session_error(&err("Invalid Token")));
        assert!(is_session_error(&err("Token expired [AB8051]")));
        assert!(!is_session_error(&err("Something Went Wrong [AB1004]")));
        assert!(!is_session_error(&err("Invalid totp [AB1050]")));
    }

    #[test]
    fn error_text_shows_the_broker_message() {
        let broker = ServerFnError::ServerError("Invalid price [AB4008]".into());
        assert_eq!(error_text(&broker), "Invalid price [AB4008]");
        let offline = ServerFnError::Request("connection refused".into());
        assert_eq!(
            error_text(&offline),
            "Can't reach the trading server. Check that it is running."
        );
    }
}
