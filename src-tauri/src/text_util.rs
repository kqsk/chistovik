//! Ported from Handy (MIT), see NOTICE.

/// Keep the original text's leading and trailing whitespace around the
/// processed result. LLMs trim or add surrounding whitespace, which would
/// otherwise glue a replaced selection to its neighbouring words.
pub fn preserve_surrounding_whitespace(original: &str, processed: &str) -> String {
    let body = original.trim();
    if body.is_empty() {
        return processed.to_string();
    }
    let start = original.find(body).unwrap_or(0);
    let leading = &original[..start];
    let trailing = &original[start + body.len()..];
    format!("{leading}{}{trailing}", processed.trim())
}

#[cfg(test)]
mod tests {
    use super::preserve_surrounding_whitespace;

    #[test]
    fn keeps_surrounding_whitespace_of_original() {
        assert_eq!(
            preserve_surrounding_whitespace(" teh cat ", "The cat.\n"),
            " The cat. "
        );
    }

    #[test]
    fn trims_result_when_original_has_no_padding() {
        assert_eq!(
            preserve_surrounding_whitespace("teh cat", "\nThe cat.\n"),
            "The cat."
        );
    }
}
