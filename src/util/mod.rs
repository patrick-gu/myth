pub(crate) trait StrExt {
    fn trim_spaces_tabs(&self) -> &str;
}

impl StrExt for str {
    fn trim_spaces_tabs(&self) -> &str {
        self.trim_matches(&[' ', '\t'] as &[char])
    }
}
