//! Words every message for the user shares.

/// `text` as a sentence: a capital letter first and a full stop last, which
/// text that ends with one keeps. Empty text stays empty.
pub(crate) fn sentence(text: &str) -> String {
    let mut characters = text.chars();
    let Some(first) = characters.next() else {
        return String::new();
    };
    let mut sentence: String = first.to_uppercase().chain(characters).collect();
    if !sentence.ends_with('.') {
        sentence.push('.');
    }
    sentence
}

#[cfg(test)]
mod tests {
    use super::sentence;

    #[test]
    fn a_sentence_starts_with_a_capital_and_ends_with_one_full_stop() {
        assert_eq!(sentence("add an action"), "Add an action.");
        assert_eq!(sentence("Done."), "Done.");
        assert_eq!(sentence("écran"), "Écran.");
        assert_eq!(sentence(""), "");
    }
}
