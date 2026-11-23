//! The XML trace (`PCheckerLogXmlFormatter` over an indented `XmlWriter`): one element per
//! runtime log event, written the way .NET's `XmlWriter` writes it.

/// An XML trace being built.
#[derive(Debug, Clone)]
pub(crate) struct XmlLog {
    out: String,
    /// `Task.CurrentId` of the test task that creates the main machine.
    pub task_id: u64,
}

impl XmlLog {
    pub(crate) fn new(task_id: u64) -> XmlLog {
        XmlLog { out: "<Log>".to_string(), task_id }
    }

    /// Writes `<name a="v" ... />`, or `<name ...>text</name>` when `text` is given.
    pub(crate) fn element(&mut self, name: &str, attrs: &[(&str, &str)], text: Option<&str>) {
        self.out.push_str("\n  <");
        self.out.push_str(name);
        for (k, v) in attrs {
            self.out.push(' ');
            self.out.push_str(k);
            self.out.push_str("=\"");
            escape_attr(&mut self.out, v);
            self.out.push('"');
        }
        match text {
            Some(t) if !t.is_empty() => {
                self.out.push('>');
                escape_text(&mut self.out, t);
                self.out.push_str("</");
                self.out.push_str(name);
                self.out.push('>');
            }
            _ => self.out.push_str(" />"),
        }
    }

    /// The document (closed `Log` element, no trailing newline).
    pub(crate) fn finish(mut self) -> String {
        self.out.push_str("\n</Log>");
        self.out
    }
}

fn escape_attr(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#xA;"),
            '\r' => out.push_str("&#xD;"),
            '\t' => out.push_str("&#x9;"),
            c => out.push(c),
        }
    }
}

fn escape_text(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_like_xml_writer() {
        let mut x = XmlLog::new(2);
        x.element("State", &[("id", "Main(1)"), ("state", "Init"), ("isEntry", "True")], None);
        x.element("AssertionFailure", &[], Some("<ErrorLog> a & \"b\""));
        x.element("Strategy", &[("strategy", "random")], Some("random[seed '7']"));
        assert_eq!(
            x.finish(),
            "<Log>\n  <State id=\"Main(1)\" state=\"Init\" isEntry=\"True\" />\n  \
             <AssertionFailure>&lt;ErrorLog&gt; a &amp; \"b\"</AssertionFailure>\n  \
             <Strategy strategy=\"random\">random[seed '7']</Strategy>\n</Log>"
        );
    }
}
