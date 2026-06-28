//! Minimal recursive-descent parser for the supported template subset:
//! elements (incl. self-closing), nested children, text with `{{ expr }}`
//! interpolation, static attributes, `:name` bound attributes, and `@name`
//! event handlers.

pub(crate) enum Node {
    Element(Element),
    StaticText(String),
    DynText(String),
}

pub(crate) struct Element {
    pub tag: String,
    pub attrs: Vec<Attr>,
    pub children: Vec<Node>,
}

pub(crate) enum Attr {
    Static { name: String, value: String },
    Dyn { name: String, expr: String },
    Event { name: String, handler: String },
}

pub(crate) fn parse(input: &str) -> Result<Vec<Node>, String> {
    let mut parser = Parser {
        chars: input.chars().collect(),
        pos: 0,
    };
    let nodes = parser.parse_nodes()?;
    if parser.pos != parser.chars.len() {
        return Err("unexpected trailing input".to_string());
    }
    Ok(nodes)
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    fn starts_with(&self, s: &str) -> bool {
        let target: Vec<char> = s.chars().collect();
        if self.pos + target.len() > self.chars.len() {
            return false;
        }
        self.chars[self.pos..self.pos + target.len()] == target[..]
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_whitespace()) {
            self.pos += 1;
        }
    }

    fn expect(&mut self, c: char) -> Result<(), String> {
        if self.peek() == Some(c) {
            self.pos += 1;
            Ok(())
        } else {
            Err(format!("expected {c:?}, found {:?}", self.peek()))
        }
    }

    fn read_name(&mut self) -> String {
        let mut name = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '@') {
                name.push(c);
                self.pos += 1;
            } else {
                break;
            }
        }
        name
    }

    /// Skip an HTML comment `<!-- ... -->`, assuming the cursor is at `<!--`.
    fn skip_comment(&mut self) -> Result<(), String> {
        self.pos += 4; // consume "<!--"
        while !self.starts_with("-->") {
            if self.bump().is_none() {
                return Err("unterminated comment `<!--`".to_string());
            }
        }
        self.pos += 3; // consume "-->"
        Ok(())
    }

    fn parse_nodes(&mut self) -> Result<Vec<Node>, String> {
        let mut nodes = Vec::new();
        loop {
            match self.peek() {
                None => break,
                Some('<') if self.starts_with("<!--") => self.skip_comment()?,
                Some('<') if self.starts_with("</") => break,
                Some('<') => nodes.push(Node::Element(self.parse_element()?)),
                _ => nodes.extend(self.parse_text()?),
            }
        }
        Ok(nodes)
    }

    fn parse_text(&mut self) -> Result<Vec<Node>, String> {
        let mut segments: Vec<Node> = Vec::new();
        let mut buf = String::new();
        while let Some(c) = self.peek() {
            if c == '<' {
                if self.starts_with("<!--") {
                    self.skip_comment()?;
                    continue;
                }
                break;
            }
            if self.starts_with("{{") {
                if !buf.is_empty() {
                    segments.push(Node::StaticText(std::mem::take(&mut buf)));
                }
                self.pos += 2;
                let mut expr = String::new();
                while !self.starts_with("}}") {
                    match self.bump() {
                        Some(ch) => expr.push(ch),
                        None => return Err("unterminated interpolation `{{`".to_string()),
                    }
                }
                self.pos += 2;
                segments.push(Node::DynText(expr.trim().to_string()));
            } else {
                buf.push(c);
                self.pos += 1;
            }
        }
        if !buf.is_empty() {
            segments.push(Node::StaticText(buf));
        }
        Ok(normalize_text(segments))
    }

    fn parse_element(&mut self) -> Result<Element, String> {
        self.expect('<')?;
        let tag = self.read_name();
        if tag.is_empty() {
            return Err("expected tag name".to_string());
        }
        let mut attrs = Vec::new();
        loop {
            self.skip_ws();
            match self.peek() {
                Some('/') => {
                    self.bump();
                    self.expect('>')?;
                    return Ok(Element {
                        tag,
                        attrs,
                        children: Vec::new(),
                    });
                }
                Some('>') => {
                    self.bump();
                    break;
                }
                None => return Err(format!("unterminated tag <{tag}>")),
                _ => attrs.push(self.parse_attr()?),
            }
        }
        let children = self.parse_nodes()?;
        if !self.starts_with("</") {
            return Err(format!("expected closing tag </{tag}>"));
        }
        self.pos += 2;
        let close = self.read_name();
        self.skip_ws();
        self.expect('>')?;
        if close != tag {
            return Err(format!("mismatched closing tag: <{tag}> closed by </{close}>"));
        }
        Ok(Element {
            tag,
            attrs,
            children,
        })
    }

    fn parse_attr(&mut self) -> Result<Attr, String> {
        let raw = self.read_name();
        if raw.is_empty() {
            return Err(format!("unexpected character {:?} in tag", self.peek()));
        }
        let mut value = None;
        let save = self.pos;
        self.skip_ws();
        if self.peek() == Some('=') {
            self.bump();
            self.skip_ws();
            let quote = self.bump().ok_or("expected attribute value")?;
            if quote != '"' && quote != '\'' {
                return Err("attribute value must be quoted".to_string());
            }
            let mut v = String::new();
            while let Some(c) = self.peek() {
                if c == '\\' {
                    // A backslash escapes the next character so the delimiter
                    // quote can appear inside the value (e.g. a Rust string
                    // literal in a bound expression: `:x="f(\"a\")"`). The
                    // escaped delimiter collapses to a bare quote; any other
                    // escape is preserved verbatim so Rust escapes like `\n`
                    // survive into the expression.
                    self.pos += 1;
                    match self.peek() {
                        Some(n) if n == quote => {
                            v.push(n);
                            self.pos += 1;
                        }
                        Some(n) => {
                            v.push('\\');
                            v.push(n);
                            self.pos += 1;
                        }
                        None => v.push('\\'),
                    }
                    continue;
                }
                if c == quote {
                    break;
                }
                v.push(c);
                self.pos += 1;
            }
            self.expect(quote)?;
            value = Some(v);
        } else {
            self.pos = save; // boolean attribute with no value
        }

        if let Some(name) = raw.strip_prefix(':') {
            Ok(Attr::Dyn {
                name: name.to_string(),
                expr: value.unwrap_or_default(),
            })
        } else if let Some(name) = raw.strip_prefix('@') {
            Ok(Attr::Event {
                name: name.to_string(),
                handler: value.unwrap_or_default(),
            })
        } else {
            Ok(Attr::Static {
                name: raw,
                value: value.unwrap_or_default(),
            })
        }
    }
}

/// Drop formatting whitespace runs and trim the outer edges of a text run, while
/// preserving meaningful inner spacing (e.g. `"count is "` before an interpolation).
fn normalize_text(mut segments: Vec<Node>) -> Vec<Node> {
    let all_whitespace = segments
        .iter()
        .all(|n| matches!(n, Node::StaticText(s) if s.trim().is_empty()));
    if all_whitespace {
        return Vec::new();
    }
    if let Some(Node::StaticText(s)) = segments.first_mut() {
        *s = s.trim_start().to_string();
    }
    if let Some(Node::StaticText(s)) = segments.last_mut() {
        *s = s.trim_end().to_string();
    }
    segments
        .into_iter()
        .filter(|n| !matches!(n, Node::StaticText(s) if s.is_empty()))
        .collect()
}
