/// S-expression parser for Specctra DSN/SES format.
#[derive(Debug, Clone, PartialEq)]
pub enum Sexp {
    Atom(String),
    List(Vec<Sexp>),
}

impl Sexp {
    pub fn as_atom(&self) -> Option<&str> {
        match self {
            Sexp::Atom(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[Sexp]> {
        match self {
            Sexp::List(v) => Some(v.as_slice()),
            _ => None,
        }
    }

    /// Returns the first atom in a list (the "name" / tag of the list).
    pub fn name(&self) -> Option<&str> {
        self.as_list()?.first()?.as_atom()
    }

    /// Finds the first direct child list whose name matches `key`.
    pub fn find(&self, key: &str) -> Option<&Sexp> {
        let list = self.as_list()?;
        list.iter().find(|s| s.name() == Some(key))
    }

    pub fn parse(input: &str) -> Result<Sexp, String> {
        let mut chars = input.chars().peekable();
        let items = parse_items(&mut chars)?;
        if items.len() == 1 {
            Ok(items.into_iter().next().unwrap())
        } else {
            Ok(Sexp::List(items))
        }
    }
}

type Peekable<'a> = std::iter::Peekable<std::str::Chars<'a>>;

fn skip_whitespace_and_comments(chars: &mut Peekable) {
    loop {
        // Skip whitespace
        while chars.peek().map(|c| c.is_whitespace()).unwrap_or(false) {
            chars.next();
        }
        // Check for line comments
        let next = chars.peek().copied();
        if next == Some('#') {
            // consume until newline
            while chars.peek().copied().map(|c| c != '\n').unwrap_or(false) {
                chars.next();
            }
        } else if next == Some('/') {
            // peek further
            let mut tmp = chars.clone();
            tmp.next(); // consume '/'
            if tmp.peek().copied() == Some('/') {
                while chars.peek().copied().map(|c| c != '\n').unwrap_or(false) {
                    chars.next();
                }
            } else {
                break;
            }
        } else {
            break;
        }
    }
}

fn parse_items(chars: &mut Peekable) -> Result<Vec<Sexp>, String> {
    let mut items = Vec::new();
    loop {
        skip_whitespace_and_comments(chars);
        match chars.peek().copied() {
            None | Some(')') => break,
            Some('(') => {
                chars.next(); // consume '('
                let children = parse_items(chars)?;
                skip_whitespace_and_comments(chars);
                match chars.next() {
                    Some(')') => {}
                    other => {
                        return Err(format!("Expected ')' but got {:?}", other));
                    }
                }
                items.push(Sexp::List(children));
            }
            Some('"') => {
                // Peek ahead: if `"` is immediately followed by `)` or whitespace, treat
                // it as the atom `"` (handles the Specctra `(string_quote ")` convention).
                let mut lookahead = chars.clone();
                lookahead.next(); // consume '"'
                let next_ch = lookahead.peek().copied();
                if next_ch == Some(')') || next_ch.map(|c| c.is_whitespace()).unwrap_or(true) {
                    chars.next(); // consume the '"'
                    items.push(Sexp::Atom("\"".to_string()));
                } else {
                    chars.next(); // consume opening '"'
                    let mut s = String::new();
                    loop {
                        match chars.next() {
                            None => return Err("Unterminated string literal".to_string()),
                            Some('"') => break,
                            Some('\\') => {
                                match chars.next() {
                                    Some(c) => s.push(c),
                                    None => return Err("Unterminated escape in string".to_string()),
                                }
                            }
                            Some(c) => s.push(c),
                        }
                    }
                    items.push(Sexp::Atom(s));
                }
            }
            _ => {
                let mut atom = String::new();
                loop {
                    match chars.peek().copied() {
                        None | Some('(') | Some(')') => break,
                        Some(c) if c.is_whitespace() => break,
                        Some('"') => {
                            // Quoted string embedded in atom (e.g., X14-"D-")
                            // Preserve the quotes so downstream parsers can detect the boundary.
                            chars.next(); // consume opening '"'
                            atom.push('"');
                            loop {
                                match chars.next() {
                                    None => break,
                                    Some('"') => { atom.push('"'); break; }
                                    Some('\\') => {
                                        if let Some(c) = chars.next() {
                                            atom.push(c);
                                        }
                                    }
                                    Some(c) => atom.push(c),
                                }
                            }
                        }
                        Some(c) => {
                            atom.push(c);
                            chars.next();
                        }
                    }
                }
                if !atom.is_empty() {
                    items.push(Sexp::Atom(atom));
                }
            }
        }
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_atom() {
        let s = Sexp::parse("hello").unwrap();
        assert_eq!(s.as_atom(), Some("hello"));
    }

    #[test]
    fn test_parse_list() {
        let s = Sexp::parse("(pcb test)").unwrap();
        let list = s.as_list().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].as_atom(), Some("pcb"));
        assert_eq!(list[1].as_atom(), Some("test"));
    }

    #[test]
    fn test_parse_quoted() {
        let s = Sexp::parse("(layer \"F.Cu\")").unwrap();
        let list = s.as_list().unwrap();
        assert_eq!(list[1].as_atom(), Some("F.Cu"));
    }

    #[test]
    fn test_find() {
        let s = Sexp::parse("(pcb foo (resolution um 10))").unwrap();
        let res = s.find("resolution").unwrap();
        assert_eq!(res.name(), Some("resolution"));
    }

    #[test]
    fn test_comment_hash() {
        let s = Sexp::parse("# comment\n(foo bar)").unwrap();
        assert_eq!(s.name(), Some("foo"));
    }

    #[test]
    fn test_comment_slash() {
        let s = Sexp::parse("// comment\n(foo bar)").unwrap();
        assert_eq!(s.name(), Some("foo"));
    }

    #[test]
    fn test_string_quote_directive() {
        // Specctra DSN files contain `(string_quote ")` where `"` is the value, not a string opener.
        let s = Sexp::parse("(parser (string_quote \") (host_cad \"KiCad\"))").unwrap();
        let sq = s.find("string_quote").unwrap();
        let items = sq.as_list().unwrap();
        assert_eq!(items[1].as_atom(), Some("\""),
            "string_quote value should be a bare dquote atom");
        let hc = s.find("host_cad").unwrap();
        let hc_items = hc.as_list().unwrap();
        assert_eq!(hc_items[1].as_atom(), Some("KiCad"));
    }
}
