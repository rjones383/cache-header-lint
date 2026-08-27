//! Parsing for the value of a `Cache-Control` header.
//!
//! The grammar is a comma-separated list of directives, each either a bare
//! token (`no-store`) or a token with a value (`max-age=3600`). Real-world
//! headers are inconsistent about spacing and quoting, so parsing is
//! deliberately forgiving rather than strict.

#[derive(Debug, Clone)]
pub struct Directive {
    pub name: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CacheControl {
    directives: Vec<Directive>,
}

impl CacheControl {
    pub fn parse(raw: &str) -> Self {
        let directives = raw
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(|part| match part.split_once('=') {
                Some((name, value)) => Directive {
                    name: name.trim().to_ascii_lowercase(),
                    value: Some(value.trim().trim_matches('"').to_string()),
                },
                None => Directive {
                    name: part.to_ascii_lowercase(),
                    value: None,
                },
            })
            .collect();

        CacheControl { directives }
    }

    pub fn has(&self, name: &str) -> bool {
        self.directives.iter().any(|d| d.name == name)
    }

    pub fn value_of(&self, name: &str) -> Option<&str> {
        self.directives
            .iter()
            .find(|d| d.name == name)
            .and_then(|d| d.value.as_deref())
    }

    pub fn max_age(&self) -> Option<u64> {
        self.value_of("max-age").and_then(|v| v.parse().ok())
    }

    pub fn stale_while_revalidate(&self) -> Option<u64> {
        self.value_of("stale-while-revalidate")
            .and_then(|v| v.parse().ok())
    }

    pub fn directives(&self) -> &[Directive] {
        &self.directives
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bare_and_valued_directives() {
        let cc = CacheControl::parse("no-cache, max-age=600, private");
        assert!(cc.has("no-cache"));
        assert!(cc.has("private"));
        assert_eq!(cc.max_age(), Some(600));
    }

    #[test]
    fn is_case_insensitive_on_names() {
        let cc = CacheControl::parse("Max-Age=10, NO-STORE");
        assert!(cc.has("no-store"));
        assert_eq!(cc.max_age(), Some(10));
    }

    #[test]
    fn parses_stale_while_revalidate() {
        let cc = CacheControl::parse("max-age=60, stale-while-revalidate=30");
        assert_eq!(cc.stale_while_revalidate(), Some(30));
    }
}
