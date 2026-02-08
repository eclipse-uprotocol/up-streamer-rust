use std::fs::canonicalize;
use std::path::PathBuf;
use up_rust::{UCode, UStatus, UUri};

fn format_parse_error(flag: &str, raw: &str, expected: &str, max: u128) -> String {
    format!(
        "invalid value for {flag}: '{raw}' (expected {expected} in decimal or 0x-prefixed hex, range 0..={max})"
    )
}

fn parse_unsigned(flag: &str, raw: &str, expected: &str, max: u128) -> Result<u128, String> {
    if raw.is_empty() || raw.chars().any(char::is_whitespace) || raw.contains('_') {
        return Err(format_parse_error(flag, raw, expected, max));
    }

    let parsed = if let Some(hex_digits) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X"))
    {
        if hex_digits.is_empty() {
            return Err(format_parse_error(flag, raw, expected, max));
        }
        u128::from_str_radix(hex_digits, 16)
            .map_err(|_| format_parse_error(flag, raw, expected, max))?
    } else {
        raw.parse::<u128>()
            .map_err(|_| format_parse_error(flag, raw, expected, max))?
    };

    if parsed > max {
        return Err(format_parse_error(flag, raw, expected, max));
    }

    Ok(parsed)
}

pub(crate) fn parse_u32_flag(flag: &str, raw: &str) -> Result<u32, String> {
    parse_unsigned(flag, raw, "u32", u32::MAX as u128).map(|value| value as u32)
}

pub(crate) fn parse_u16_flag(flag: &str, raw: &str) -> Result<u16, String> {
    parse_unsigned(flag, raw, "u16", u16::MAX as u128).map(|value| value as u16)
}

pub(crate) fn parse_u8_flag(flag: &str, raw: &str) -> Result<u8, String> {
    parse_unsigned(flag, raw, "u8", u8::MAX as u128).map(|value| value as u8)
}

pub(crate) fn invalid_argument_status(message: impl Into<String>) -> UStatus {
    UStatus::fail_with_code(UCode::INVALID_ARGUMENT, message.into())
}

pub(crate) fn parse_u32_status(flag: &str, raw: &str) -> Result<u32, UStatus> {
    parse_u32_flag(flag, raw).map_err(invalid_argument_status)
}

pub(crate) fn parse_u16_status(flag: &str, raw: &str) -> Result<u16, UStatus> {
    parse_u16_flag(flag, raw).map_err(invalid_argument_status)
}

pub(crate) fn parse_u8_status(flag: &str, raw: &str) -> Result<u8, UStatus> {
    parse_u8_flag(flag, raw).map_err(invalid_argument_status)
}

pub(crate) fn build_uuri(
    authority: &str,
    uentity: u32,
    uversion: u8,
    resource: u16,
) -> Result<UUri, UStatus> {
    UUri::try_from_parts(authority, uentity, uversion, resource).map_err(|error| {
        invalid_argument_status(format!(
            "unable to build UUri from authority='{authority}', uentity={uentity:#X}, uversion={uversion:#X}, resource={resource:#X}: {error}"
        ))
    })
}

pub(crate) fn canonicalize_cli_path(flag: &str, raw: &str) -> Result<PathBuf, UStatus> {
    canonicalize(PathBuf::from(raw)).map_err(|error| {
        invalid_argument_status(format!(
            "invalid value for {flag}: '{raw}' (expected an existing file path): {error}"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, Parser};

    #[test]
    fn parse_u32_accepts_decimal_and_hex() {
        assert_eq!(parse_u32_flag("--uentity", "23456").unwrap(), 23_456);
        assert_eq!(parse_u32_flag("--uentity", "0x5BA0").unwrap(), 0x5BA0);
    }

    #[test]
    fn parse_u8_rejects_underscores_and_whitespace() {
        let underscore_error = parse_u8_flag("--uversion", "0x1_0").unwrap_err();
        assert_eq!(
            underscore_error,
            "invalid value for --uversion: '0x1_0' (expected u8 in decimal or 0x-prefixed hex, range 0..=255)"
        );

        let whitespace_error = parse_u8_flag("--uversion", "1 0").unwrap_err();
        assert_eq!(
            whitespace_error,
            "invalid value for --uversion: '1 0' (expected u8 in decimal or 0x-prefixed hex, range 0..=255)"
        );
    }

    #[test]
    fn parse_u16_reports_range_overflow() {
        let error = parse_u16_flag("--resource", "0x1_0000").unwrap_err();
        assert_eq!(
            error,
            "invalid value for --resource: '0x1_0000' (expected u16 in decimal or 0x-prefixed hex, range 0..=65535)"
        );
    }

    #[test]
    fn build_uuri_from_parts() {
        let uuri = build_uuri("authority-a", 0x5BA0, 0x1, 0x8001).unwrap();
        assert_eq!(uuri.authority_name, "authority-a");
        assert_eq!(uuri.ue_id, 0x5BA0);
        assert_eq!(uuri.uentity_major_version(), 0x1);
        assert_eq!(uuri.resource_id(), 0x8001);
    }

    #[derive(Debug, Parser)]
    #[command(version, about, long_about = None)]
    struct RepresentativeHelpArgs {
        #[arg(long, default_value = "authority-a")]
        uauthority: String,
        #[arg(long, default_value = "0x5BA0")]
        uentity: String,
        #[arg(long, default_value = "0x1")]
        uversion: String,
        #[arg(long, default_value = "0x8001")]
        resource: String,
    }

    #[test]
    fn representative_help_includes_common_flags_and_defaults() {
        let mut command = RepresentativeHelpArgs::command();
        let help = command.render_long_help().to_string();

        assert!(help.contains("--uauthority <UAUTHORITY>"));
        assert!(help.contains("authority-a"));
        assert!(help.contains("--uentity <UENTITY>"));
        assert!(help.contains("0x5BA0"));
        assert!(help.contains("--uversion <UVERSION>"));
        assert!(help.contains("--resource <RESOURCE>"));
    }
}
