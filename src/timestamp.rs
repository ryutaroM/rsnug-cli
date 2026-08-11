use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

pub fn format(seconds: u64) -> String {
    i64::try_from(seconds)
        .ok()
        .and_then(|seconds| OffsetDateTime::from_unix_timestamp(seconds).ok())
        .and_then(|moment| moment.format(&Rfc3339).ok())
        .unwrap_or_else(|| seconds.to_string())
}

pub fn parse(text: &str) -> Result<u64, String> {
    let moment = OffsetDateTime::parse(text, &Rfc3339)
        .map_err(|_| format!("`{text}` is not an RFC3339 timestamp"))?;
    u64::try_from(moment.unix_timestamp()).map_err(|_| format!("`{text}` is before the Unix epoch"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_renders_rfc3339() {
        assert_eq!(format(1_704_067_200), "2024-01-01T00:00:00Z");
    }

    #[test]
    fn parse_reads_what_format_wrote() {
        for seconds in [0, 1, 1_700_000_000, 1_704_067_200, 4_102_444_800] {
            assert_eq!(parse(&format(seconds)), Ok(seconds));
        }
    }

    #[test]
    fn parse_rejects_a_non_timestamp() {
        assert!(parse("yesterday").is_err());
    }

    #[test]
    fn parse_rejects_a_pre_epoch_timestamp() {
        assert!(parse("1969-01-01T00:00:00Z").is_err());
    }
}
