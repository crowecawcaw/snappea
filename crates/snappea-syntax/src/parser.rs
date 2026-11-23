//! Recursive descent parser for P, equivalent to `PParser.g4`.

use crate::ast::*;
use crate::lexer::error_display;
use crate::span::{FileId, Span};
use crate::token::TokenKind as T;
use crate::token::{Token, TokenKind};

/// A syntax error. `message` follows the ANTLR style used upstream
/// (`mismatched input 'x' expecting 'y'`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub span: Span,
    pub message: String,
}

type PResult<T> = Result<T, ParseError>;

/// The deepest nesting of expressions, statements, types and module expressions
/// [`parse`] accepts. Each nested construct counts one level, and so does each
/// operand of an operator chain (`a + b + c`, `a.b.c`, `x as T as U`), since a
/// chain builds a tree as deep as it is long. The parser and every later pass
/// recurse over that depth, so deeper input is a syntax error instead of a
/// stack overflow. Real models stay far below the limit.
pub const MAX_NESTING: usize = 128;

/// Parses one file into a [`Program`].
pub fn parse(file: FileId, text: &str) -> PResult<Program> {
    let tokens = crate::lexer::lex(file, text).map_err(|e| ParseError { span: e.span, message: e.message })?;
    parse_tokens(text, tokens)
}

/// [`parse`] for a file already lexed: `tokens` are [`crate::lexer::lex`]'s for `text`.
pub fn parse_tokens(text: &str, tokens: Vec<crate::token::Token>) -> PResult<Program> {
    Parser { text, tokens, pos: 0, depth: 0, too_deep: false }.program()
}

/// Tokens that can start a statement (`statement` in `PParser.g4`).
const STMT_STARTS: &[TokenKind] = &[
    T::LBrace,
    T::Assert,
    T::Assume,
    T::Print,
    T::Return,
    T::Break,
    T::Continue,
    T::Iden,
    T::While,
    T::Foreach,
    T::If,
    T::New,
    T::Raise,
    T::Send,
    T::Announce,
    T::Goto,
    T::Receive,
    T::Semi,
];

/// Tokens that can start a top-level declaration (`topDecl`).
const TOP_DECL_STARTS: &[TokenKind] = &[
    T::Type,
    T::Enum,
    T::Event,
    T::EventSet,
    T::Interface,
    T::Machine,
    T::Spec,
    T::Scenario,
    T::Fun,
    T::Pure,
    T::Module,
    T::Test,
    T::Implementation,
    T::Param,
    T::Invariant,
    T::Lemma,
    T::Theorem,
    T::Axiom,
    T::InitCondition,
    T::Proof,
];

/// Tokens that can start a machine body entry (`machineEntry`).
const MACHINE_ENTRY_STARTS: &[TokenKind] = &[T::Var, T::Fun, T::Start, T::Hot, T::Cold, T::State];

/// Tokens that can start a state body item (`stateBodyItem`).
const STATE_ITEM_STARTS: &[TokenKind] = &[T::Defer, T::Entry, T::Exit, T::Ignore, T::On];

/// Appends the comments in `gap` (text between two tokens), which ANTLR keeps
/// on its hidden channel and includes in "no viable alternative" input text.
fn push_comments(out: &mut String, gap: &str) {
    let mut rest = gap;
    while let Some(i) = rest.find('/') {
        let tail = &rest[i..];
        if tail.starts_with("//") {
            let end = tail.find(['\r', '\n']).unwrap_or(tail.len());
            out.push_str(&tail[..end]);
            rest = &tail[end..];
        } else if let Some(body) = tail.strip_prefix("/*") {
            let end = body.find("*/").map(|j| j + 4).unwrap_or(tail.len());
            out.push_str(&tail[..end]);
            rest = &tail[end..];
        } else {
            rest = &tail[1..];
        }
    }
}

struct Parser<'a> {
    text: &'a str,
    tokens: Vec<Token>,
    pos: usize,
    /// Current nesting level; see [`MAX_NESTING`].
    depth: usize,
    /// Set when the nesting limit is hit, so the error is not reworded.
    too_deep: bool,
}

impl<'a> Parser<'a> {
    // ----- token helpers -----

    fn peek(&self) -> Token {
        self.tokens[self.pos]
    }

    fn peek_kind(&self) -> TokenKind {
        self.tokens[self.pos].kind
    }

    fn peek_at(&self, n: usize) -> TokenKind {
        let i = (self.pos + n).min(self.tokens.len() - 1);
        self.tokens[i].kind
    }

    fn prev_span(&self) -> Span {
        self.tokens[self.pos.saturating_sub(1)].span
    }

    fn bump(&mut self) -> Token {
        let t = self.tokens[self.pos];
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        t
    }

    fn at(&self, k: TokenKind) -> bool {
        self.peek_kind() == k
    }

    fn eat(&mut self, k: TokenKind) -> bool {
        if self.at(k) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn tok_text(&self, t: Token) -> &'a str {
        &self.text[t.span.start as usize..t.span.end as usize]
    }

    fn err_at<R>(&self, tok: Token, message: String) -> PResult<R> {
        Err(ParseError { span: tok.span, message })
    }

    fn offending(&self, tok: Token) -> String {
        if tok.kind == T::Eof {
            "<EOF>".to_string()
        } else {
            error_display(self.tok_text(tok))
        }
    }

    /// Formats a token set the way ANTLR's `IntervalSet.ToString(vocabulary)`
    /// does: ordered by token type (`<EOF>`, type -1, first), braces only around several elements.
    fn format_set(kinds: &[TokenKind]) -> String {
        let mut ks: Vec<TokenKind> = kinds.to_vec();
        ks.sort_by_key(|k| (*k != T::Eof, *k as u32));
        ks.dedup();
        let names: Vec<&str> = ks.iter().map(|k| if *k == T::Eof { "<EOF>" } else { k.display() }).collect();
        if names.len() == 1 {
            names[0].to_string()
        } else {
            format!("{{{}}}", names.join(", "))
        }
    }

    /// Reports a token that does not match `expected`, following the order of
    /// ANTLR's `RecoverInline`: when the next token is expected the current one
    /// is "extraneous"; when the current one can follow the expected token
    /// (`follow`), the expected token is "missing"; otherwise a plain mismatch.
    fn mismatch<R>(&self, expected: &[TokenKind], follow: &[TokenKind]) -> PResult<R> {
        let t = self.peek();
        let set = Self::format_set(expected);
        if t.kind != T::Eof && expected.contains(&self.peek_at(1)) {
            return self.err_at(t, format!("extraneous input '{}' expecting {}", self.offending(t), set));
        }
        if follow.contains(&t.kind) {
            return self.err_at(t, format!("missing {} at '{}'", set, self.offending(t)));
        }
        self.err_at(t, format!("mismatched input '{}' expecting {}", self.offending(t), set))
    }

    fn expect(&mut self, k: TokenKind) -> PResult<Token> {
        if self.at(k) {
            Ok(self.bump())
        } else {
            self.mismatch(&[k], &[])
        }
    }

    /// `expect(k)` where the tokens in `follow` may come right after `k`.
    fn expect_followed_by(&mut self, k: TokenKind, follow: &[TokenKind]) -> PResult<Token> {
        if self.at(k) {
            Ok(self.bump())
        } else {
            self.mismatch(&[k], follow)
        }
    }

    /// The `;` ending a statement; a statement or the end of the block follows.
    fn expect_stmt_semi(&mut self) -> PResult<Token> {
        let mut follow = STMT_STARTS.to_vec();
        follow.push(T::RBrace);
        follow.push(T::Else);
        self.expect_followed_by(T::Semi, &follow)
    }

    /// The `;` ending a top-level declaration; another declaration or the end of the
    /// file follows.
    fn expect_top_semi(&mut self) -> PResult<Token> {
        let mut follow = TOP_DECL_STARTS.to_vec();
        follow.push(T::Eof);
        self.expect_followed_by(T::Semi, &follow)
    }

    /// The `;` ending a state body item; another item or the end of the state follows.
    fn expect_state_item_semi(&mut self) -> PResult<Token> {
        let mut follow = STATE_ITEM_STARTS.to_vec();
        follow.push(T::RBrace);
        self.expect_followed_by(T::Semi, &follow)
    }

    /// The error-recovery `Sync` ANTLR runs before each iteration of an
    /// `item* terminator` loop. Tokens that can neither start an item nor end
    /// the loop are reported against the set of both; on the loop's first
    /// iteration the report is a mismatch unless the token after it is
    /// acceptable, afterwards always an "extraneous input".
    fn sync_loop(&self, starts: &[TokenKind], terminator: TokenKind, first: bool) -> PResult<()> {
        let k = self.peek_kind();
        if k == terminator || starts.contains(&k) {
            return Ok(());
        }
        let mut set = starts.to_vec();
        set.push(terminator);
        let t = self.peek();
        let msg = Self::format_set(&set);
        if !first || set.contains(&self.peek_at(1)) {
            self.err_at(t, format!("extraneous input '{}' expecting {}", self.offending(t), msg))
        } else {
            self.err_at(t, format!("mismatched input '{}' expecting {}", self.offending(t), msg))
        }
    }

    /// Enters one level of nesting, failing past [`MAX_NESTING`]. Callers undo it
    /// with [`Self::leave`], or by restoring `depth` after an operator chain.
    /// Errors end the parse, so they need not undo it.
    fn enter(&mut self) -> PResult<()> {
        self.depth += 1;
        if self.depth > MAX_NESTING {
            self.too_deep = true;
            let t = self.peek();
            return self.err_at(t, format!("nesting is too deep (more than {MAX_NESTING} levels)"));
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    fn no_viable<R>(&self) -> PResult<R> {
        let t = self.peek();
        self.err_at(t, format!("no viable alternative at input '{}'", self.offending(t)))
    }

    /// Turns an error raised while a multi-token decision starting at token
    /// `start` was still open into ANTLR's "no viable alternative" report,
    /// whose input text runs from the decision start through the offending
    /// token (comments included, whitespace dropped).
    fn no_viable_from(&self, start: usize, err: ParseError) -> ParseError {
        if self.too_deep {
            return err;
        }
        let Some(end) = self.tokens.iter().position(|t| t.span.start == err.span.start && t.span.end == err.span.end)
        else {
            return err;
        };
        if self.tokens[start].kind == T::Eof {
            return ParseError { span: err.span, message: "no viable alternative at input '<EOF>'".into() };
        }
        let mut text = String::new();
        for i in start..=end {
            let t = self.tokens[i];
            if t.kind == T::Eof {
                break;
            }
            if i > start {
                push_comments(&mut text, &self.text[self.tokens[i - 1].span.end as usize..t.span.start as usize]);
            }
            text.push_str(self.tok_text(t));
        }
        let text = error_display(&text);
        ParseError { span: err.span, message: format!("no viable alternative at input '{}'", text) }
    }

    fn ident(&mut self) -> PResult<Ident> {
        let t = self.expect(T::Iden)?;
        Ok(Ident { name: self.tok_text(t).to_string(), span: t.span })
    }

    fn ident_list(&mut self) -> PResult<Vec<Ident>> {
        let mut v = vec![self.ident()?];
        while self.eat(T::Comma) {
            v.push(self.ident()?);
        }
        Ok(v)
    }

    // ----- program -----

    fn program(&mut self) -> PResult<Program> {
        let mut decls = Vec::new();
        self.sync_loop(TOP_DECL_STARTS, T::Eof, true)?;
        while !self.at(T::Eof) {
            decls.push(self.top_decl()?);
            self.sync_loop(TOP_DECL_STARTS, T::Eof, false)?;
        }
        Ok(Program { decls })
    }

    fn top_decl(&mut self) -> PResult<TopDecl> {
        let start = self.peek().span;
        match self.peek_kind() {
            T::Type => {
                self.bump();
                let name = self.ident()?;
                let ty = if self.eat(T::Assign) { Some(self.ty()?) } else { None };
                let end = if ty.is_some() { self.expect_top_semi()? } else { self.expect(T::Semi)? }.span;
                Ok(TopDecl::TypeDef { name, ty, span: start.to(end) })
            }
            T::Enum => {
                self.bump();
                let name = self.ident()?;
                self.expect(T::LBrace)?;
                let mut elems = Vec::new();
                let numbered = self.peek_at(1) == T::Assign;
                loop {
                    let n = self.ident()?;
                    let value = if numbered {
                        self.expect(T::Assign)?;
                        let v = self.expect(T::IntLiteral)?;
                        Some((self.tok_text(v).to_string(), v.span))
                    } else {
                        None
                    };
                    elems.push(EnumElem { name: n, value });
                    if !self.eat(T::Comma) {
                        break;
                    }
                }
                let end = self.expect(T::RBrace)?.span;
                Ok(TopDecl::Enum { name, elems, span: start.to(end) })
            }
            T::Event => {
                self.bump();
                let name = self.ident()?;
                let payload = if self.eat(T::Colon) {
                    Some(self.ty()?)
                } else if !self.at(T::Semi) {
                    return self.mismatch(&[T::Semi, T::Colon], &[]);
                } else {
                    None
                };
                let end = self.expect_top_semi()?.span;
                Ok(TopDecl::Event { name, payload, span: start.to(end) })
            }
            T::EventSet => {
                self.bump();
                let name = self.ident()?;
                self.expect(T::Assign)?;
                self.expect(T::LBrace)?;
                let events = self.non_default_event_list()?;
                self.expect(T::RBrace)?;
                let end = self.expect_top_semi()?.span;
                Ok(TopDecl::EventSet { name, events, span: start.to(end) })
            }
            T::Interface => {
                self.bump();
                let name = self.ident()?;
                self.expect(T::LParen)?;
                let payload = if self.at(T::RParen) { None } else { Some(self.ty()?) };
                self.expect(T::RParen)?;
                let receives = if self.eat(T::Receives) {
                    if self.at(T::Semi) {
                        Some(Vec::new())
                    } else {
                        Some(self.non_default_event_list()?)
                    }
                } else {
                    None
                };
                let end = self.expect_top_semi()?.span;
                Ok(TopDecl::Interface { name, payload, receives, span: start.to(end) })
            }
            T::Machine => self.machine_decl(MachineKind::Impl).map(TopDecl::Machine),
            T::Spec => self.machine_decl(MachineKind::Spec).map(TopDecl::Machine),
            T::Scenario => self.machine_decl(MachineKind::Scenario).map(TopDecl::Machine),
            T::Fun => self.fun_decl().map(TopDecl::Fun),
            T::Pure => {
                self.bump();
                let name = self.ident()?;
                self.expect(T::LParen)?;
                let params = if self.at(T::RParen) { Vec::new() } else { self.fun_param_list()? };
                self.expect(T::RParen)?;
                self.expect(T::Colon)?;
                let ret = self.ty()?;
                let body = if self.eat(T::Assign) { Some(self.expr()?) } else { None };
                let end = self.expect(T::Semi)?.span;
                Ok(TopDecl::Pure(PureDecl { name, params, ret, body, span: start.to(end) }))
            }
            T::Module => {
                self.bump();
                let name = self.ident()?;
                self.expect(T::Assign)?;
                let expr = self.mod_expr()?;
                let end = self.expect_top_semi()?.span;
                Ok(TopDecl::Module { name, expr, span: start.to(end) })
            }
            T::Test => self.test_decl(),
            T::Implementation => {
                self.bump();
                let name = self.ident()?;
                let main = if self.eat(T::LBrack) {
                    self.expect(T::Main)?;
                    self.expect(T::Assign)?;
                    let m = self.ident()?;
                    self.expect(T::RBrack)?;
                    Some(m)
                } else {
                    None
                };
                self.expect(T::Colon)?;
                let expr = self.mod_expr()?;
                let end = self.expect_top_semi()?.span;
                Ok(TopDecl::Implementation { name, main, expr, span: start.to(end) })
            }
            T::Param => {
                self.bump();
                let names = self.ident_list()?;
                self.expect(T::Colon)?;
                let ty = self.ty()?;
                let end = self.expect_top_semi()?.span;
                Ok(TopDecl::GlobalParam { names, ty, span: start.to(end) })
            }
            T::Invariant => self.invariant_decl().map(TopDecl::Invariant),
            T::Lemma | T::Theorem => {
                let is_theorem = self.bump().kind == T::Theorem;
                let name = self.ident()?;
                self.expect(T::LBrace)?;
                let mut invariants = Vec::new();
                while self.at(T::Invariant) {
                    invariants.push(self.invariant_decl()?);
                }
                let end = self.expect(T::RBrace)?.span;
                Ok(TopDecl::InvariantGroup { name, is_theorem, invariants, span: start.to(end) })
            }
            T::Axiom => {
                self.bump();
                let body = self.expr()?;
                let end = self.expect(T::Semi)?.span;
                Ok(TopDecl::Axiom { body, span: start.to(end) })
            }
            T::InitCondition => {
                self.bump();
                let body = self.expr()?;
                let end = self.expect(T::Semi)?.span;
                Ok(TopDecl::AssumeOnStart { body, span: start.to(end) })
            }
            T::Proof => {
                self.bump();
                let name = if self.at(T::Iden) { Some(self.ident()?) } else { None };
                self.expect(T::LBrace)?;
                let mut items = Vec::new();
                while self.at(T::Prove) {
                    items.push(self.proof_item()?);
                }
                let end = self.expect(T::RBrace)?.span;
                Ok(TopDecl::ProofBlock { name, items, span: start.to(end) })
            }
            _ => self.no_viable(),
        }
    }

    fn invariant_decl(&mut self) -> PResult<InvariantDecl> {
        let start = self.expect(T::Invariant)?.span;
        let name = self.ident()?;
        self.expect(T::Colon)?;
        let body = self.expr()?;
        let end = self.expect(T::Semi)?.span;
        Ok(InvariantDecl { name, body, span: start.to(end) })
    }

    fn proof_item(&mut self) -> PResult<ProofItem> {
        let start = self.expect(T::Prove)?.span;
        let mut item = ProofItem {
            targets: Vec::new(),
            targets_all: false,
            targets_default: false,
            premises: Vec::new(),
            premises_all: false,
            excludes: Vec::new(),
            span: start,
        };
        if self.eat(T::Mul) {
            item.targets_all = true;
        } else if self.eat(T::Default) {
            item.targets_default = true;
        } else {
            item.targets.push(self.expr()?);
            while self.eat(T::Comma) {
                item.targets.push(self.expr()?);
            }
        }
        if self.eat(T::Using) {
            if self.eat(T::Mul) {
                item.premises_all = true;
            } else {
                item.premises.push(self.expr()?);
                while self.eat(T::Comma) {
                    item.premises.push(self.expr()?);
                }
            }
        }
        if self.eat(T::Except) {
            item.excludes.push(self.expr()?);
            while self.eat(T::Comma) {
                item.excludes.push(self.expr()?);
            }
        }
        let end = self.expect(T::Semi)?.span;
        item.span = start.to(end);
        Ok(item)
    }

    fn test_decl(&mut self) -> PResult<TopDecl> {
        let start = self.expect(T::Test)?.span;
        // refinement test: TEST name [main=M] : modExpr REFINES modExpr ;
        // safety test: TEST (PARAM param)? (ASSUME expr)? twise? name [main=M] : modExpr ;
        let mut params = None;
        let mut assume = None;
        let mut twise = None;
        if self.at(T::Param) {
            let pstart = self.bump().span;
            self.expect(T::LParen)?;
            let mut bindings = Vec::new();
            loop {
                let bstart = self.peek().span;
                let name = self.ident()?;
                self.expect(T::In)?;
                let values = self.seq_literal()?;
                bindings.push(ParamBinding { name, values, span: bstart.to(self.prev_span()) });
                if !self.eat(T::Comma) {
                    break;
                }
            }
            let pend = self.expect(T::RParen)?.span;
            params = Some((bindings, pstart.to(pend)));
        }
        if self.eat(T::Assume) {
            assume = Some(self.expr()?);
        }
        if self.at(T::Pairwise) {
            twise = Some(Twise::Pairwise(self.bump().span));
        } else if self.at(T::LParen) && self.peek_at(1) == T::IntLiteral && self.peek_at(2) == T::Wise {
            let s = self.bump().span;
            let n = self.bump();
            self.bump();
            let e = self.expect(T::RParen)?.span;
            twise = Some(Twise::N(self.tok_text(n).to_string(), s.to(e)));
        }
        let name = self.ident()?;
        self.expect(T::LBrack)?;
        self.expect(T::Main)?;
        self.expect(T::Assign)?;
        let main = self.ident()?;
        self.expect(T::RBrack)?;
        self.expect(T::Colon)?;
        let expr = self.mod_expr()?;
        if self.at(T::Refines) {
            if params.is_some() || assume.is_some() || twise.is_some() {
                return self.no_viable();
            }
            self.bump();
            let rhs = self.mod_expr()?;
            let end = self.expect(T::Semi)?.span;
            return Ok(TopDecl::RefinementTest { name, main, lhs: expr, rhs, span: start.to(end) });
        }
        let end = self.expect(T::Semi)?.span;
        Ok(TopDecl::SafetyTest(SafetyTestDecl { name, main, expr, params, assume, twise, span: start.to(end) }))
    }

    fn seq_literal(&mut self) -> PResult<Vec<SeqPrimitive>> {
        self.expect(T::LBrack)?;
        let mut v = Vec::new();
        loop {
            let t = self.peek();
            match t.kind {
                T::BoolLiteral => {
                    self.bump();
                    v.push(SeqPrimitive::Bool(self.tok_text(t) == "true", t.span));
                }
                T::IntLiteral => {
                    self.bump();
                    v.push(SeqPrimitive::Int { negative: false, text: self.tok_text(t).to_string(), span: t.span });
                }
                T::Sub => {
                    self.bump();
                    let n = self.expect(T::IntLiteral)?;
                    v.push(SeqPrimitive::Int {
                        negative: true,
                        text: self.tok_text(n).to_string(),
                        span: t.span.to(n.span),
                    });
                }
                _ => return self.no_viable(),
            }
            if !self.eat(T::Comma) {
                break;
            }
        }
        self.expect(T::RBrack)?;
        Ok(v)
    }

    // ----- module expressions -----

    fn mod_expr(&mut self) -> PResult<ModExpr> {
        self.enter()?;
        let e = self.mod_expr_inner()?;
        self.leave();
        Ok(e)
    }

    fn mod_expr_inner(&mut self) -> PResult<ModExpr> {
        let start = self.peek().span;
        let kind = match self.peek_kind() {
            T::LParen => {
                self.bump();
                let inner = self.mod_expr()?;
                self.expect(T::RParen)?;
                ModExprKind::Paren(Box::new(inner))
            }
            T::LBrace => {
                self.bump();
                let mut binds = Vec::new();
                loop {
                    let bstart = self.peek().span;
                    let machine = self.ident()?;
                    let interface = if self.eat(T::RArrow) { Some(self.ident()?) } else { None };
                    binds.push(BindExpr { machine, interface, span: bstart.to(self.prev_span()) });
                    if !self.eat(T::Comma) {
                        break;
                    }
                }
                self.expect(T::RBrace)?;
                ModExprKind::Bind(binds)
            }
            T::Iden => ModExprKind::Named(self.ident()?),
            T::Compose | T::Union => {
                let is_compose = self.bump().kind == T::Compose;
                let mut mods = vec![self.mod_expr()?];
                self.expect(T::Comma)?;
                mods.push(self.mod_expr()?);
                while self.eat(T::Comma) {
                    mods.push(self.mod_expr()?);
                }
                if is_compose {
                    ModExprKind::Compose(mods)
                } else {
                    ModExprKind::Union(mods)
                }
            }
            T::HideE => {
                self.bump();
                let events = self.non_default_event_list()?;
                self.expect(T::In)?;
                let module = Box::new(self.mod_expr()?);
                ModExprKind::HideEvents { events, module }
            }
            T::HideI => {
                self.bump();
                let interfaces = self.ident_list()?;
                self.expect(T::In)?;
                let module = Box::new(self.mod_expr()?);
                ModExprKind::HideInterfaces { interfaces, module }
            }
            T::Assert => {
                self.bump();
                let monitors = self.ident_list()?;
                self.expect(T::In)?;
                let module = Box::new(self.mod_expr()?);
                ModExprKind::Assert { monitors, module }
            }
            T::Rename => {
                self.bump();
                let old = self.ident()?;
                self.expect(T::To)?;
                let new = self.ident()?;
                self.expect(T::In)?;
                let module = Box::new(self.mod_expr()?);
                ModExprKind::Rename { old, new, module }
            }
            T::Main => {
                self.bump();
                let main = self.ident()?;
                self.expect(T::In)?;
                let module = Box::new(self.mod_expr()?);
                ModExprKind::Main { main, module }
            }
            _ => return self.no_viable(),
        };
        Ok(ModExpr { kind, span: start.to(self.prev_span()) })
    }

    // ----- machines -----

    fn machine_decl(&mut self, kind: MachineKind) -> PResult<MachineDecl> {
        let start = self.bump().span;
        let name = self.ident()?;
        let mut receives = Vec::new();
        let mut sends = Vec::new();
        let mut observes = Vec::new();
        match kind {
            MachineKind::Impl => {
                self.sync_loop(&[T::Receives, T::Sends], T::LBrace, true)?;
                while self.at(T::Receives) || self.at(T::Sends) {
                    let s = self.peek().span;
                    let is_recv = self.bump().kind == T::Receives;
                    let events = if self.at(T::Semi) { Vec::new() } else { self.non_default_event_list()? };
                    let e = self.expect_followed_by(T::Semi, &[T::Receives, T::Sends, T::LBrace])?.span;
                    if is_recv {
                        receives.push((events, s.to(e)));
                    } else {
                        sends.push((events, s.to(e)));
                    }
                    self.sync_loop(&[T::Receives, T::Sends], T::LBrace, false)?;
                }
            }
            MachineKind::Spec | MachineKind::Scenario => {
                self.expect(T::Observes)?;
                observes = self.non_default_event_list()?;
            }
        }
        self.expect(T::LBrace)?;
        let mut vars = Vec::new();
        let mut funs = Vec::new();
        let mut states = Vec::new();
        let mut first = true;
        let mut var_follow = MACHINE_ENTRY_STARTS.to_vec();
        var_follow.push(T::RBrace);
        loop {
            self.sync_loop(MACHINE_ENTRY_STARTS, T::RBrace, first)?;
            first = false;
            match self.peek_kind() {
                T::Var => vars.push(self.var_decl(&var_follow)?),
                T::Fun => funs.push(self.fun_decl()?),
                T::Start | T::Hot | T::Cold | T::State => states.push(self.state_decl()?),
                T::RBrace => break,
                _ => return self.no_viable(),
            }
        }
        let end = self.expect(T::RBrace)?.span;
        Ok(MachineDecl { kind, name, receives, sends, observes, vars, funs, states, span: start.to(end) })
    }

    /// `var x, y: T;`, where the tokens in `follow` can come after the declaration.
    fn var_decl(&mut self, follow: &[TokenKind]) -> PResult<VarDecl> {
        let start = self.expect(T::Var)?.span;
        let names = self.ident_list()?;
        self.expect(T::Colon)?;
        let ty = self.ty()?;
        let end = self.expect_followed_by(T::Semi, follow)?.span;
        Ok(VarDecl { names, ty, span: start.to(end) })
    }

    fn fun_param(&mut self) -> PResult<Param> {
        let start = self.peek().span;
        let name = self.ident()?;
        self.expect(T::Colon)?;
        let ty = self.ty()?;
        Ok(Param { name, ty, span: start.to(self.prev_span()) })
    }

    fn fun_param_list(&mut self) -> PResult<Vec<Param>> {
        let mut v = vec![self.fun_param()?];
        while self.eat(T::Comma) {
            v.push(self.fun_param()?);
        }
        Ok(v)
    }

    fn fun_decl(&mut self) -> PResult<FunDecl> {
        let start = self.expect(T::Fun)?.span;
        let name = self.ident()?;
        self.expect(T::LParen)?;
        let params = if self.at(T::RParen) { Vec::new() } else { self.fun_param_list()? };
        self.expect(T::RParen)?;
        let mut decl = FunDecl {
            name,
            params,
            ret: None,
            body: None,
            creates: Vec::new(),
            return_param: None,
            requires: Vec::new(),
            ensures: Vec::new(),
            span: start,
        };
        if self.at(T::Return) {
            // foreign with verifier contract: RETURN (param) ; (REQUIRES e;)* (ENSURES e;)*
            self.bump();
            self.expect(T::LParen)?;
            decl.return_param = Some(self.fun_param()?);
            self.expect(T::RParen)?;
            self.expect(T::Semi)?;
            self.requires_ensures(&mut decl)?;
            decl.span = start.to(self.prev_span());
            return Ok(decl);
        }
        if self.eat(T::Colon) {
            decl.ret = Some(self.ty()?);
        }
        if self.at(T::LBrace) {
            decl.body = Some(self.fun_body()?);
            decl.span = start.to(self.prev_span());
            return Ok(decl);
        }
        if self.eat(T::Creates) {
            decl.creates.push(self.ident()?);
        }
        if self.at(T::Semi) {
            self.bump();
            decl.span = start.to(self.prev_span());
            return Ok(decl);
        }
        if decl.ret.is_none() && decl.creates.is_empty() && (self.at(T::Requires) || self.at(T::Ensures)) {
            self.requires_ensures(&mut decl)?;
            decl.span = start.to(self.prev_span());
            return Ok(decl);
        }
        let t = self.peek();
        self.err_at(t, format!("mismatched input '{}' expecting {{';', '{{', 'creates'}}", self.offending(t)))
    }

    fn requires_ensures(&mut self, decl: &mut FunDecl) -> PResult<()> {
        while self.eat(T::Requires) {
            decl.requires.push(self.expr()?);
            self.expect(T::Semi)?;
        }
        while self.eat(T::Ensures) {
            decl.ensures.push(self.expr()?);
            self.expect(T::Semi)?;
        }
        Ok(())
    }

    fn fun_body(&mut self) -> PResult<FunBody> {
        let start = self.expect(T::LBrace)?.span;
        let mut vars = Vec::new();
        let var_starts: Vec<TokenKind> = std::iter::once(T::Var).chain(STMT_STARTS.iter().copied()).collect();
        let mut var_follow = var_starts.clone();
        var_follow.push(T::RBrace);
        self.sync_loop(&var_starts, T::RBrace, true)?;
        while self.at(T::Var) {
            vars.push(self.var_decl(&var_follow)?);
            self.sync_loop(&var_starts, T::RBrace, false)?;
        }
        let mut stmts = Vec::new();
        while !self.at(T::RBrace) {
            stmts.push(self.stmt()?);
            self.sync_loop(STMT_STARTS, T::RBrace, false)?;
        }
        let end = self.expect(T::RBrace)?.span;
        Ok(FunBody { vars, stmts, span: start.to(end) })
    }

    fn anon_handler(&mut self, allow_param: bool) -> PResult<AnonHandler> {
        let start = self.peek().span;
        let param = if allow_param && self.at(T::LParen) {
            self.bump();
            let p = self.fun_param()?;
            self.expect(T::RParen)?;
            Some(p)
        } else {
            None
        };
        let body = self.fun_body()?;
        Ok(AnonHandler { param, body, span: start.to(self.prev_span()) })
    }

    fn state_decl(&mut self) -> PResult<StateDecl> {
        let start = self.peek().span;
        let is_start = self.eat(T::Start);
        let temperature = if self.eat(T::Hot) {
            Temperature::Hot
        } else if self.eat(T::Cold) {
            Temperature::Cold
        } else {
            Temperature::Warm
        };
        self.expect(T::State)?;
        let name = self.ident()?;
        self.expect(T::LBrace)?;
        let mut items = Vec::new();
        self.sync_loop(STATE_ITEM_STARTS, T::RBrace, true)?;
        while !self.at(T::RBrace) {
            items.push(self.state_item()?);
            self.sync_loop(STATE_ITEM_STARTS, T::RBrace, false)?;
        }
        let end = self.expect(T::RBrace)?.span;
        Ok(StateDecl { name, is_start, temperature, items, span: start.to(end) })
    }

    fn handler_or_named(&mut self, allow_param: bool) -> PResult<Handler> {
        if self.at(T::Iden) {
            let name = self.ident()?;
            self.expect_state_item_semi()?;
            Ok(Handler::Named(name))
        } else {
            Ok(Handler::Anon(self.anon_handler(allow_param)?))
        }
    }

    fn state_item(&mut self) -> PResult<StateItem> {
        let start = self.peek().span;
        let first_tok = self.pos;
        let kind = match self.peek_kind() {
            T::Entry => {
                self.bump();
                StateItemKind::Entry(self.handler_or_named(true)?)
            }
            T::Exit => {
                self.bump();
                StateItemKind::Exit(self.handler_or_named(false)?)
            }
            T::Defer => {
                self.bump();
                let events = self.non_default_event_list()?;
                self.expect_state_item_semi()?;
                StateItemKind::Defer(events)
            }
            T::Ignore => {
                self.bump();
                let events = self.non_default_event_list()?;
                self.expect_state_item_semi()?;
                StateItemKind::Ignore(events)
            }
            T::On => {
                self.bump();
                let events = self.event_list()?;
                if self.eat(T::Do) {
                    let handler = self.handler_or_named(true)?;
                    StateItemKind::OnDo { events, handler }
                } else {
                    self.expect(T::Goto)?;
                    let target = self.ident()?;
                    let with = if self.eat(T::With) {
                        Some(self.handler_or_named(true)?)
                    } else if self.eat(T::Semi) {
                        None
                    } else {
                        // ANTLR is still choosing among the `on ... goto` items here.
                        return self.no_viable().map_err(|e| self.no_viable_from(first_tok, e));
                    };
                    StateItemKind::OnGoto { events, target, with }
                }
            }
            _ => return self.no_viable(),
        };
        Ok(StateItem { kind, span: start.to(self.prev_span()) })
    }

    fn non_default_event(&mut self) -> PResult<Ident> {
        let t = self.peek();
        match t.kind {
            T::Halt => {
                self.bump();
                Ok(Ident { name: "halt".to_string(), span: t.span })
            }
            T::Iden => self.ident(),
            _ => self.mismatch(&[T::Halt, T::Iden], &[]),
        }
    }

    fn non_default_event_list(&mut self) -> PResult<Vec<Ident>> {
        let mut v = vec![self.non_default_event()?];
        while self.eat(T::Comma) {
            v.push(self.non_default_event()?);
        }
        Ok(v)
    }

    fn event_id(&mut self) -> PResult<Ident> {
        let t = self.peek();
        match t.kind {
            T::NullLiteral => {
                self.bump();
                Ok(Ident { name: "null".to_string(), span: t.span })
            }
            T::Halt => {
                self.bump();
                Ok(Ident { name: "halt".to_string(), span: t.span })
            }
            T::Iden => self.ident(),
            _ => self.mismatch(&[T::NullLiteral, T::Halt, T::Iden], &[]),
        }
    }

    fn event_list(&mut self) -> PResult<Vec<Ident>> {
        let mut v = vec![self.event_id()?];
        while self.eat(T::Comma) {
            v.push(self.event_id()?);
        }
        Ok(v)
    }

    // ----- types -----

    fn ty(&mut self) -> PResult<Type> {
        self.enter()?;
        let t = self.ty_inner()?;
        self.leave();
        Ok(t)
    }

    fn ty_inner(&mut self) -> PResult<Type> {
        let start = self.peek().span;
        let kind = match self.peek_kind() {
            T::Seq => {
                self.bump();
                self.expect(T::LBrack)?;
                let t = self.ty()?;
                self.expect(T::RBrack)?;
                TypeKind::Seq(Box::new(t))
            }
            T::Set => {
                self.bump();
                self.expect(T::LBrack)?;
                let t = self.ty()?;
                self.expect(T::RBrack)?;
                TypeKind::Set(Box::new(t))
            }
            T::Map => {
                self.bump();
                self.expect(T::LBrack)?;
                let k = self.ty()?;
                self.expect(T::Comma)?;
                let v = self.ty()?;
                self.expect(T::RBrack)?;
                TypeKind::Map(Box::new(k), Box::new(v))
            }
            T::LParen => {
                self.bump();
                if self.at(T::Iden) && self.peek_at(1) == T::Colon {
                    let mut fields = Vec::new();
                    loop {
                        let n = self.ident()?;
                        self.expect(T::Colon)?;
                        let t = self.ty()?;
                        fields.push((n, t));
                        if !self.eat(T::Comma) {
                            break;
                        }
                    }
                    self.expect(T::RParen)?;
                    TypeKind::NamedTuple(fields)
                } else {
                    let mut tys = vec![self.ty()?];
                    while self.eat(T::Comma) {
                        tys.push(self.ty()?);
                    }
                    self.expect(T::RParen)?;
                    TypeKind::Tuple(tys)
                }
            }
            T::Bool => {
                self.bump();
                TypeKind::Bool
            }
            T::Int => {
                self.bump();
                TypeKind::Int
            }
            T::Float => {
                self.bump();
                TypeKind::Float
            }
            T::String => {
                self.bump();
                TypeKind::String
            }
            T::Event => {
                self.bump();
                TypeKind::Event
            }
            T::Machine => {
                self.bump();
                TypeKind::Machine
            }
            T::Data => {
                self.bump();
                TypeKind::Data
            }
            T::Any => {
                self.bump();
                TypeKind::Any
            }
            T::Iden => TypeKind::Named(self.ident()?),
            _ => return self.no_viable(),
        };
        Ok(Type { kind, span: start.to(self.prev_span()) })
    }

    // ----- statements -----

    /// A statement. Only the ones that contain statements are parsed in this
    /// frame, one of which is on the stack per level of statement nesting; the
    /// others go to [`Self::simple_stmt`], which keeps their locals out of it.
    fn stmt(&mut self) -> PResult<Stmt> {
        self.enter()?;
        let start = self.peek().span;
        let kind = match self.peek_kind() {
            T::LBrace => {
                self.bump();
                let mut stmts = Vec::new();
                self.sync_loop(STMT_STARTS, T::RBrace, true)?;
                while !self.at(T::RBrace) {
                    stmts.push(self.stmt()?);
                    self.sync_loop(STMT_STARTS, T::RBrace, false)?;
                }
                self.expect(T::RBrace)?;
                StmtKind::Compound(stmts)
            }
            T::While => {
                self.bump();
                self.expect(T::LParen)?;
                let cond = self.expr()?;
                self.expect(T::RParen)?;
                let body = Box::new(self.stmt()?);
                StmtKind::While { cond, body }
            }
            T::Foreach => self.foreach_stmt()?,
            T::If => {
                self.bump();
                self.expect(T::LParen)?;
                let cond = self.expr()?;
                self.expect(T::RParen)?;
                let then_branch = Box::new(self.stmt()?);
                let else_branch = if self.eat(T::Else) { Some(Box::new(self.stmt()?)) } else { None };
                StmtKind::If { cond, then_branch, else_branch }
            }
            T::Receive => self.receive_stmt()?,
            _ => self.simple_stmt()?,
        };
        self.leave();
        Ok(Stmt { kind, span: start.to(self.prev_span()) })
    }

    #[inline(never)]
    fn foreach_stmt(&mut self) -> PResult<StmtKind> {
        self.bump();
        self.expect(T::LParen)?;
        let item = self.ident()?;
        self.expect(T::In)?;
        let collection = self.expr()?;
        self.expect(T::RParen)?;
        let mut invariants = Vec::new();
        while self.eat(T::Invariant) {
            invariants.push(self.expr()?);
            self.expect_stmt_semi()?;
        }
        let body = Box::new(self.stmt()?);
        Ok(StmtKind::Foreach { item, collection, invariants, body })
    }

    #[inline(never)]
    fn receive_stmt(&mut self) -> PResult<StmtKind> {
        self.bump();
        self.expect(T::LBrace)?;
        let mut cases = Vec::new();
        while self.at(T::Case) {
            let cstart = self.bump().span;
            let events = self.event_list()?;
            self.expect(T::Colon)?;
            let handler = self.anon_handler(true)?;
            cases.push(RecvCase { events, handler, span: cstart.to(self.prev_span()) });
        }
        if cases.is_empty() {
            self.expect(T::Case)?;
        }
        self.expect(T::RBrace)?;
        Ok(StmtKind::Receive { cases })
    }

    #[inline(never)]
    fn simple_stmt(&mut self) -> PResult<StmtKind> {
        Ok(match self.peek_kind() {
            T::Assert | T::Assume => {
                let is_assert = self.bump().kind == T::Assert;
                let cond = self.expr()?;
                let message = if self.eat(T::Comma) { Some(self.expr()?) } else { None };
                self.expect_stmt_semi()?;
                if is_assert {
                    StmtKind::Assert { cond, message }
                } else {
                    StmtKind::Assume { cond, message }
                }
            }
            T::Print => {
                self.bump();
                let e = self.expr()?;
                self.expect_stmt_semi()?;
                StmtKind::Print(e)
            }
            T::Return => {
                self.bump();
                let e = if self.at(T::Semi) { None } else { Some(self.expr()?) };
                self.expect_stmt_semi()?;
                StmtKind::Return(e)
            }
            T::Break => {
                self.bump();
                self.expect_stmt_semi()?;
                StmtKind::Break
            }
            T::Continue => {
                self.bump();
                self.expect_stmt_semi()?;
                StmtKind::Continue
            }
            T::New => {
                self.bump();
                let interface = self.ident()?;
                self.expect(T::LParen)?;
                let args = if self.at(T::RParen) { Vec::new() } else { self.rvalue_list()? };
                self.expect(T::RParen)?;
                self.expect_stmt_semi()?;
                StmtKind::Ctor { interface, args }
            }
            T::Raise => {
                self.bump();
                let event = self.expr()?;
                let args = if self.eat(T::Comma) { self.rvalue_list()? } else { Vec::new() };
                self.expect_stmt_semi()?;
                StmtKind::Raise { event, args }
            }
            T::Send => {
                self.bump();
                let target = self.expr()?;
                self.expect(T::Comma)?;
                let event = self.expr()?;
                let args = if self.eat(T::Comma) { self.rvalue_list()? } else { Vec::new() };
                self.expect_stmt_semi()?;
                StmtKind::Send { target, event, args }
            }
            T::Announce => {
                self.bump();
                let event = self.expr()?;
                let args = if self.eat(T::Comma) { self.rvalue_list()? } else { Vec::new() };
                self.expect_stmt_semi()?;
                StmtKind::Announce { event, args }
            }
            T::Goto => {
                self.bump();
                let state = self.ident()?;
                let args = if self.eat(T::Comma) { self.rvalue_list()? } else { Vec::new() };
                self.expect_stmt_semi()?;
                StmtKind::Goto { state, args }
            }
            T::Semi => {
                self.bump();
                StmtKind::NoStmt
            }
            T::Iden => {
                if self.peek_at(1) == T::LParen {
                    let fun = self.ident()?;
                    self.expect(T::LParen)?;
                    let args = if self.at(T::RParen) { Vec::new() } else { self.rvalue_list()? };
                    self.expect(T::RParen)?;
                    self.expect_stmt_semi()?;
                    StmtKind::FunCall { fun, args }
                } else {
                    // ANTLR predicts among the assignment-like statements with
                    // unbounded lookahead, so an error before the alternative
                    // is decided is a "no viable alternative" at the statement.
                    let first_tok = self.pos;
                    let prefix = (|| -> PResult<(Expr, TokenKind, Option<Expr>)> {
                        let lhs = self.lvalue()?;
                        let op = self.peek_kind();
                        match op {
                            T::Assign | T::Remove => {
                                self.bump();
                                Ok((lhs, op, None))
                            }
                            T::Insert => {
                                self.bump();
                                self.expect(T::LParen)?;
                                let first = self.expr()?;
                                if !self.at(T::Comma) && !self.at(T::RParen) {
                                    return self.no_viable();
                                }
                                Ok((lhs, op, Some(first)))
                            }
                            _ => self.no_viable(),
                        }
                    })()
                    .map_err(|e| self.no_viable_from(first_tok, e))?;
                    match prefix {
                        (lhs, T::Assign, _) => {
                            let rhs = self.expr()?;
                            self.expect_stmt_semi()?;
                            StmtKind::Assign { lhs, rhs }
                        }
                        (lhs, T::Remove, _) => {
                            let value = self.expr()?;
                            self.expect_stmt_semi()?;
                            StmtKind::Remove { lhs, value }
                        }
                        (lhs, _, first) => {
                            let first = first.expect("insert prefix has a first operand");
                            if self.eat(T::Comma) {
                                let value = self.expr()?;
                                self.expect(T::RParen)?;
                                self.expect_stmt_semi()?;
                                StmtKind::Insert { lhs, index: first, value }
                            } else {
                                self.expect(T::RParen)?;
                                self.expect_stmt_semi()?;
                                StmtKind::Add { lhs, value: first }
                            }
                        }
                    }
                }
            }
            _ => return self.no_viable(),
        })
    }

    fn lvalue(&mut self) -> PResult<Expr> {
        let start = self.peek().span;
        let name = self.ident()?;
        let mut e = Expr { kind: ExprKind::Name(name), span: start, inner: start };
        let base = self.depth;
        loop {
            if matches!(self.peek_kind(), T::Dot | T::LBrack) {
                self.enter()?;
            }
            match self.peek_kind() {
                T::Dot => {
                    self.bump();
                    if self.at(T::IntLiteral) {
                        let t = self.bump();
                        e = Expr {
                            kind: ExprKind::TupleAccess {
                                base: Box::new(e),
                                index: self.tok_text(t).to_string(),
                                index_span: t.span,
                            },
                            span: start.to(t.span),
                            inner: start.to(t.span),
                        };
                    } else {
                        let field = self.ident()?;
                        let sp = start.to(field.span);
                        e = Expr { kind: ExprKind::NamedTupleAccess { base: Box::new(e), field }, span: sp, inner: sp };
                    }
                }
                T::LBrack => {
                    self.bump();
                    let index = self.expr()?;
                    let end = self.expect(T::RBrack)?.span;
                    e = Expr {
                        kind: ExprKind::Index { base: Box::new(e), index: Box::new(index) },
                        span: start.to(end),
                        inner: start.to(end),
                    };
                }
                _ => break,
            }
        }
        self.depth = base;
        Ok(e)
    }

    fn rvalue_list(&mut self) -> PResult<Vec<Expr>> {
        let mut v = vec![self.expr()?];
        while self.eat(T::Comma) {
            v.push(self.expr()?);
        }
        Ok(v)
    }

    // ----- expressions -----
    //
    // Precedence (highest first), mirroring the alternative order in PParser.g4:
    //   postfix: `.f`, `.n`, `is`, `targets`, `[i]`
    //   prefix: inflight, sent, -, !
    //   * / %
    //   + -
    //   as / to (postfix cast)
    //   < > >= <= in
    //   == !=
    //   &&
    //   ||
    //   ==>
    //   <==>

    fn expr(&mut self) -> PResult<Expr> {
        self.enter()?;
        let e = self.binary(0)?;
        self.leave();
        Ok(e)
    }

    /// A binary operator and its precedence level, 0 (`<==>`) to 8 (`*`).
    /// Level 6 is the cast (`as`/`to`), which [`Self::binary`] handles itself.
    fn binop(kind: TokenKind) -> Option<(BinOp, u8)> {
        Some(match kind {
            T::LIff => (BinOp::Iff, 0),
            T::LThen => (BinOp::Then, 1),
            T::LOr => (BinOp::Or, 2),
            T::LAnd => (BinOp::And, 3),
            T::Eq => (BinOp::Eq, 4),
            T::Ne => (BinOp::Ne, 4),
            T::Lt => (BinOp::Lt, 5),
            T::Gt => (BinOp::Gt, 5),
            T::Ge => (BinOp::Ge, 5),
            T::Le => (BinOp::Le, 5),
            T::In => (BinOp::In, 5),
            T::Add => (BinOp::Add, 7),
            T::Sub => (BinOp::Sub, 7),
            T::Mul => (BinOp::Mul, 8),
            T::Div => (BinOp::Div, 8),
            T::Mod => (BinOp::Mod, 8),
            _ => return None,
        })
    }

    /// The operators of level `min_level` and tighter, all left-associative, by
    /// precedence climbing. This builds the same tree as one recursive function
    /// per level would, with one stack frame per operand instead of one per level.
    fn binary(&mut self, min_level: u8) -> PResult<Expr> {
        const CAST_LEVEL: u8 = 6;
        let mut lhs = self.unary()?;
        let base = self.depth;
        // An operator binds no tighter than the one before it: the tighter ones
        // went into that operator's right operand, and a cast takes no operand.
        let mut max_level = u8::MAX;
        loop {
            let t = self.peek();
            let (op, level) = match t.kind {
                T::As | T::To => (None, CAST_LEVEL),
                k => match Self::binop(k) {
                    Some((op, level)) => (Some(op), level),
                    None => break,
                },
            };
            if level < min_level || level > max_level {
                break;
            }
            max_level = level;
            self.enter()?;
            self.bump();
            let Some(op) = op else {
                let kind = if t.kind == T::As { CastKind::As } else { CastKind::To };
                let ty = self.ty()?;
                let sp = lhs.span.to(ty.span);
                lhs = Expr { kind: ExprKind::Cast { kind, expr: Box::new(lhs), ty }, span: sp, inner: sp };
                continue;
            };
            let rhs = self.binary(level + 1)?;
            let sp = lhs.span.to(rhs.span);
            lhs = Expr {
                kind: ExprKind::Binary { op, op_span: t.span, lhs: Box::new(lhs), rhs: Box::new(rhs) },
                span: sp,
                inner: sp,
            };
        }
        self.depth = base;
        Ok(lhs)
    }

    fn unary(&mut self) -> PResult<Expr> {
        let t = self.peek();
        if !matches!(t.kind, T::Sub | T::LNot | T::Inflight | T::Sent) {
            return self.postfix();
        }
        self.bump();
        self.enter()?;
        let e = Box::new(self.unary()?);
        self.leave();
        let sp = t.span.to(e.span);
        let kind = match t.kind {
            T::Sub => ExprKind::Unary { op: UnOp::Neg, operand: e },
            T::LNot => ExprKind::Unary { op: UnOp::Not, operand: e },
            T::Inflight => ExprKind::Flying(e),
            _ => ExprKind::Sent(e),
        };
        Ok(Expr { kind, span: sp, inner: sp })
    }

    fn postfix(&mut self) -> PResult<Expr> {
        let mut e = self.primary()?;
        let start = e.span;
        let base = self.depth;
        loop {
            if matches!(self.peek_kind(), T::Dot | T::Is | T::Targets | T::LBrack) {
                self.enter()?;
            }
            match self.peek_kind() {
                T::Dot => {
                    self.bump();
                    if self.at(T::IntLiteral) {
                        let t = self.bump();
                        e = Expr {
                            kind: ExprKind::TupleAccess {
                                base: Box::new(e),
                                index: self.tok_text(t).to_string(),
                                index_span: t.span,
                            },
                            span: start.to(t.span),
                            inner: start.to(t.span),
                        };
                    } else {
                        let field = self.ident()?;
                        let sp = start.to(field.span);
                        e = Expr { kind: ExprKind::NamedTupleAccess { base: Box::new(e), field }, span: sp, inner: sp };
                    }
                }
                T::Is => {
                    self.bump();
                    let kind = self.ident()?;
                    let sp = start.to(kind.span);
                    e = Expr { kind: ExprKind::Test { instance: Box::new(e), kind }, span: sp, inner: sp };
                }
                T::Targets => {
                    self.bump();
                    let target = self.postfix()?;
                    let sp = start.to(target.span);
                    e = Expr {
                        kind: ExprKind::Targets { instance: Box::new(e), target: Box::new(target) },
                        span: sp,
                        inner: sp,
                    };
                }
                T::LBrack => {
                    self.bump();
                    let index = self.expr()?;
                    let end = self.expect(T::RBrack)?.span;
                    e = Expr {
                        kind: ExprKind::Index { base: Box::new(e), index: Box::new(index) },
                        span: start.to(end),
                        inner: start.to(end),
                    };
                }
                _ => break,
            }
        }
        self.depth = base;
        Ok(e)
    }

    fn args_in_parens(&mut self) -> PResult<Vec<Expr>> {
        self.expect(T::LParen)?;
        let args = if self.at(T::RParen) { Vec::new() } else { self.rvalue_list()? };
        self.expect(T::RParen)?;
        Ok(args)
    }

    /// A primary expression. Parentheses, the most deeply nested primaries, are
    /// parsed apart from the others so that each kind of nesting costs only the
    /// stack it needs.
    fn primary(&mut self) -> PResult<Expr> {
        if self.at(T::LParen) {
            self.paren_expr()
        } else {
            self.primary_other()
        }
    }

    /// A parenthesized expression, an unnamed tuple or a named tuple.
    #[inline(never)]
    fn paren_expr(&mut self) -> PResult<Expr> {
        let start = self.bump().span;
        let kind = if self.at(T::Iden) && self.peek_at(1) == T::Assign {
            self.named_tuple_body()?
        } else {
            let first = self.expr()?;
            if self.eat(T::Comma) {
                // `unnamedTupleBody`: a trailing comma only after a single field.
                let mut fields = vec![first];
                if !self.at(T::RParen) {
                    loop {
                        fields.push(self.expr()?);
                        if !self.eat(T::Comma) {
                            break;
                        }
                    }
                }
                self.expect(T::RParen)?;
                ExprKind::UnnamedTuple(fields)
            } else {
                self.expect(T::RParen)?;
                // parenthesized expression: keep inner node, widen span
                return Ok(Expr { kind: first.kind, span: start.to(self.prev_span()), inner: first.inner });
            }
        };
        let span = start.to(self.prev_span());
        Ok(Expr { kind, span, inner: span })
    }

    #[inline(never)]
    fn named_tuple_body(&mut self) -> PResult<ExprKind> {
        // `namedTupleBody`: either one field and a comma, or several fields
        // without a trailing comma. ANTLR picks the alternative by looking
        // past the first comma, so an error before it is a "no viable
        // alternative" from the first name; after it, anything but a name
        // is taken as the one-field form.
        let first_tok = self.pos;
        let first = (|| -> PResult<(Ident, Expr)> {
            let n = self.ident()?;
            self.expect(T::Assign)?;
            let v = self.expr()?;
            if !self.eat(T::Comma) {
                return self.no_viable();
            }
            Ok((n, v))
        })()
        .map_err(|e| self.no_viable_from(first_tok, e))?;
        let mut fields = vec![first];
        if self.at(T::Iden) {
            loop {
                let n = self.ident()?;
                self.expect(T::Assign)?;
                fields.push((n, self.expr()?));
                if !self.eat(T::Comma) {
                    break;
                }
            }
        }
        self.expect(T::RParen)?;
        Ok(ExprKind::NamedTuple(fields))
    }

    #[inline(never)]
    fn primary_other(&mut self) -> PResult<Expr> {
        let t = self.peek();
        let start = t.span;
        let kind = match t.kind {
            T::Keys | T::Values | T::Sizeof => {
                self.bump();
                self.expect(T::LParen)?;
                let e = Box::new(self.expr()?);
                self.expect(T::RParen)?;
                match t.kind {
                    T::Keys => ExprKind::Keys(e),
                    T::Values => ExprKind::Values(e),
                    _ => ExprKind::Sizeof(e),
                }
            }
            T::Default => {
                self.bump();
                self.expect(T::LParen)?;
                let ty = self.ty()?;
                self.expect(T::RParen)?;
                ExprKind::Default(ty)
            }
            T::New => {
                self.bump();
                let interface = self.ident()?;
                let args = self.args_in_parens()?;
                ExprKind::Ctor { interface, args }
            }
            T::Iden => {
                if self.peek_at(1) == T::LParen {
                    let fun = self.ident()?;
                    let args = self.args_in_parens()?;
                    ExprKind::FunCall { fun, args }
                } else {
                    ExprKind::Name(self.ident()?)
                }
            }
            T::Forall | T::Exists => {
                self.bump();
                let quant = if t.kind == T::Forall { Quantifier::Forall } else { Quantifier::Exists };
                let diff = self.eat(T::New);
                self.expect(T::LParen)?;
                let bound = self.fun_param_list()?;
                self.expect(T::RParen)?;
                self.expect(T::Colon)?;
                self.expect(T::Colon)?;
                let body = Box::new(self.expr()?);
                ExprKind::Quant { quant, diff, bound, body }
            }
            T::Choose => {
                self.bump();
                self.expect(T::LParen)?;
                let e = if self.at(T::RParen) { None } else { Some(Box::new(self.expr()?)) };
                self.expect(T::RParen)?;
                ExprKind::Choose(e)
            }
            T::StringLiteral => {
                self.bump();
                ExprKind::Str { raw: self.tok_text(t).to_string(), raw_span: t.span, args: Vec::new() }
            }
            T::Format => {
                self.bump();
                self.expect(T::LParen)?;
                let s = self.expect(T::StringLiteral)?;
                let args = if self.eat(T::Comma) { self.rvalue_list()? } else { Vec::new() };
                self.expect(T::RParen)?;
                ExprKind::Str { raw: self.tok_text(s).to_string(), raw_span: s.span, args }
            }
            T::Float => {
                // float(base, exp)
                self.bump();
                self.expect(T::LParen)?;
                let b = self.expect(T::IntLiteral)?;
                self.expect(T::Comma)?;
                let e = self.expect(T::IntLiteral)?;
                self.expect(T::RParen)?;
                ExprKind::FloatLit(format!("{}e{}", self.tok_text(b), self.tok_text(e)))
            }
            T::IntLiteral => {
                self.bump();
                if self.at(T::Dot) && self.peek_at(1) == T::IntLiteral {
                    self.bump();
                    let post = self.bump();
                    ExprKind::FloatLit(format!("{}.{}", self.tok_text(t), self.tok_text(post)))
                } else {
                    ExprKind::IntLit(self.tok_text(t).to_string())
                }
            }
            T::Dot => {
                // `.5`
                self.bump();
                let post = self.expect(T::IntLiteral)?;
                ExprKind::FloatLit(format!(".{}", self.tok_text(post)))
            }
            T::BoolLiteral => {
                self.bump();
                ExprKind::BoolLit(self.tok_text(t) == "true")
            }
            T::NullLiteral => {
                self.bump();
                ExprKind::NullLit
            }
            T::Nondet => {
                self.bump();
                ExprKind::Nondet
            }
            T::FairNondet => {
                self.bump();
                ExprKind::FairNondet
            }
            T::Halt => {
                self.bump();
                ExprKind::Halt
            }
            T::This => {
                self.bump();
                ExprKind::This
            }
            _ => return self.no_viable(),
        };
        let span = start.to(self.prev_span());
        Ok(Expr { kind, span, inner: span })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_precedence() {
        let p = parse(FileId(0), "machine M { start state S { entry { x = -a.0 * b + c as int < d && e || f; } } }")
            .unwrap();
        let TopDecl::Machine(m) = &p.decls[0] else { panic!() };
        let StateItemKind::Entry(Handler::Anon(h)) = &m.states[0].items[0].kind else { panic!() };
        let StmtKind::Assign { rhs, .. } = &h.body.stmts[0].kind else { panic!() };
        let ExprKind::Binary { op: BinOp::Or, lhs, .. } = &rhs.kind else { panic!("{:?}", rhs.kind) };
        let ExprKind::Binary { op: BinOp::And, lhs, .. } = &lhs.kind else { panic!() };
        let ExprKind::Binary { op: BinOp::Lt, lhs, .. } = &lhs.kind else { panic!() };
        let ExprKind::Cast { expr, .. } = &lhs.kind else { panic!() };
        let ExprKind::Binary { op: BinOp::Add, lhs, .. } = &expr.kind else { panic!() };
        let ExprKind::Binary { op: BinOp::Mul, lhs, .. } = &lhs.kind else { panic!() };
        let ExprKind::Unary { op: UnOp::Neg, operand } = &lhs.kind else { panic!() };
        assert!(matches!(operand.kind, ExprKind::TupleAccess { .. }));
    }

    #[test]
    fn parses_tuples_and_floats() {
        let p = parse(FileId(0), "fun f() { x = (1,); y = (a=1, b=2.5); z = (1); w = .5; }").unwrap();
        let TopDecl::Fun(f) = &p.decls[0] else { panic!() };
        let b = f.body.as_ref().unwrap();
        assert!(
            matches!(&b.stmts[0].kind, StmtKind::Assign { rhs, .. } if matches!(rhs.kind, ExprKind::UnnamedTuple(_)))
        );
        assert!(
            matches!(&b.stmts[1].kind, StmtKind::Assign { rhs, .. } if matches!(rhs.kind, ExprKind::NamedTuple(_)))
        );
        assert!(matches!(&b.stmts[2].kind, StmtKind::Assign { rhs, .. } if matches!(rhs.kind, ExprKind::IntLit(_))));
        assert!(matches!(&b.stmts[3].kind, StmtKind::Assign { rhs, .. } if matches!(rhs.kind, ExprKind::FloatLit(_))));
    }

    fn error(src: &str) -> String {
        parse(FileId(0), src).unwrap_err().message
    }

    #[test]
    fn rejects_malformed_tuples() {
        // Messages from ANTLR 4 running upstream's grammar in LL mode, except the last,
        // which follows this parser's report for an expression that cannot start.
        assert_eq!(error("fun f() { x = (a = 1); }"), "no viable alternative at input 'a=1)'");
        assert_eq!(error("fun f() { x = (a = 1 2); }"), "no viable alternative at input 'a=12'");
        assert_eq!(error("fun f() { x = (a=1, b=2,); }"), "mismatched input ')' expecting Iden");
        assert_eq!(error("fun f() { x = (a=1, 2); }"), "extraneous input '2' expecting ')'");
        assert_eq!(error("fun f() { x = (1, 2,); }"), "no viable alternative at input ')'");
        assert!(parse(FileId(0), "fun f() { x = (a=1,); y = (1,); z = (a=1, b=2); w = (1, 2); }").is_ok());
    }

    #[test]
    fn reports_declaration_errors() {
        // Messages from ANTLR 4 running upstream's grammar in LL mode.
        let cases = [
            ("machine M { var x: int }", "missing ';' at '}'"),
            ("fun f() { var x: int x = 1; }", "missing ';' at 'x'"),
            ("event e : int\nevent f;", "missing ';' at 'event'"),
            ("event e\nevent f;", "mismatched input 'event' expecting {';', ':'}"),
            ("type T; type U = int", "missing ';' at '<EOF>'"),
            ("type T = int }", "mismatched input '}' expecting ';'"),
            ("eventset s = { e } event f;", "missing ';' at 'event'"),
            ("interface I() receives e event f;", "missing ';' at 'event'"),
            ("param x: int event f;", "missing ';' at 'event'"),
            ("module m = {M} event e;", "missing ';' at 'event'"),
            ("implementation I [main=M]: {M} event f;", "missing ';' at 'event'"),
            ("machine M receives e { }", "missing ';' at '{'"),
            ("machine M { start state S { entry Foo } }", "missing ';' at '}'"),
            ("machine M { start state S { on e do F } }", "missing ';' at '}'"),
            ("machine M { start state S { on e goto S with F } }", "missing ';' at '}'"),
            ("machine M { start state S { ignore e, f exit { } } }", "missing ';' at 'exit'"),
            ("machine M { start state S { on e goto S } }", "no viable alternative at input 'onegotoS}'"),
            ("foo event", "extraneous input 'foo' expecting {<EOF>, 'enum', 'event', 'eventset', 'machine', 'interface', 'fun', 'scenario', 'spec', 'type', 'param', 'invariant', 'axiom', 'Proof', 'Lemma', 'Theorem', 'init-condition', 'pure', 'module', 'implementation', 'test'}"),
            ("fun f() {} ) x", "extraneous input ')' expecting {<EOF>, 'enum', 'event', 'eventset', 'machine', 'interface', 'fun', 'scenario', 'spec', 'type', 'param', 'invariant', 'axiom', 'Proof', 'Lemma', 'Theorem', 'init-condition', 'pure', 'module', 'implementation', 'test'}"),
            ("x = 1;", "mismatched input 'x' expecting {<EOF>, 'enum', 'event', 'eventset', 'machine', 'interface', 'fun', 'scenario', 'spec', 'type', 'param', 'invariant', 'axiom', 'Proof', 'Lemma', 'Theorem', 'init-condition', 'pure', 'module', 'implementation', 'test'}"),
            ("fun f() {} \"a\tb\"", "extraneous input '\"a\\tb\"' expecting {<EOF>, 'enum', 'event', 'eventset', 'machine', 'interface', 'fun', 'scenario', 'spec', 'type', 'param', 'invariant', 'axiom', 'Proof', 'Lemma', 'Theorem', 'init-condition', 'pure', 'module', 'implementation', 'test'}"),
            ("event e x;", "extraneous input 'x' expecting {';', ':'}"),
            ("event e x", "mismatched input 'x' expecting {';', ':'}"),
        ];
        for (src, want) in cases {
            assert_eq!(error(src), want, "{src}");
        }
    }

    /// Runs `f` on a stack of the size the WebAssembly build has (1 MiB) or, for
    /// the much larger frames of a debug build, a native main thread's (8 MiB).
    fn on_small_stack(f: impl FnOnce() + Send + 'static) {
        let size = if cfg!(debug_assertions) { 8 << 20 } else { 1 << 20 };
        std::thread::Builder::new().stack_size(size).spawn(f).unwrap().join().unwrap();
    }

    fn too_deep(src: &str) -> bool {
        parse(FileId(0), src)
            .is_err_and(|e| e.message == format!("nesting is too deep (more than {MAX_NESTING} levels)"))
    }

    #[test]
    fn deep_nesting_is_a_syntax_error() {
        on_small_stack(deep_nesting_cases);
    }

    fn deep_nesting_cases() {
        // Each of these used to overflow the stack, in the parser or in a later pass.
        let n = 2000;
        let fun = |body: String| format!("fun f() {{ {body} }}");
        let cases = [
            fun(format!("x = {}1{};", "(".repeat(n), ")".repeat(n))),
            fun(format!("x = 1{};", " + 1".repeat(20_000))),
            fun(format!("x = 1{};", " as int".repeat(n))),
            fun(format!("x = {}1;", "-".repeat(n))),
            fun(format!("x = {}1{};", "f(".repeat(n), ")".repeat(n))),
            fun(format!("x = {}0{};", "s[".repeat(n), "]".repeat(n))),
            fun(format!("x = t{};", ".0".repeat(n))),
            fun(format!("x{} = 1;", "[0]".repeat(n))),
            fun(format!("{}x = 1;{}", "{".repeat(n), "}".repeat(n))),
            fun(format!("{}x = 1;", "if (b) ".repeat(n))),
            fun(format!("{}x = 1;", "if (b) x = 1; else ".repeat(n))),
            fun(format!("{}x = 1;", "while (b) ".repeat(n))),
            format!("type T = {}int{};", "seq[".repeat(n), "]".repeat(n)),
            format!("module m = {}{{M}}{};", "(".repeat(n), ")".repeat(n)),
        ];
        for src in &cases {
            assert!(too_deep(src), "{}", &src[..60]);
        }
    }

    #[test]
    fn nesting_up_to_the_limit_parses() {
        on_small_stack(nesting_up_to_the_limit_cases);
    }

    fn nesting_up_to_the_limit_cases() {
        // The statement and the expression on its right count two levels.
        let parens = |k: usize| format!("fun f() {{ x = {}1{}; }}", "(".repeat(k), ")".repeat(k));
        assert!(parse(FileId(0), &parens(MAX_NESTING - 2)).is_ok());
        assert!(too_deep(&parens(MAX_NESTING - 1)));
        // Each operator of a chain counts one level.
        let chain = |k: usize| format!("fun f() {{ x = 1{}; }}", " + 1".repeat(k));
        assert!(parse(FileId(0), &chain(MAX_NESTING - 2)).is_ok());
        assert!(too_deep(&chain(MAX_NESTING - 1)));
        // So does each block, and the statement and expression inside.
        let deep = format!("fun f() {{ {}x = 1;{} }}", "{".repeat(MAX_NESTING - 2), "}".repeat(MAX_NESTING - 2));
        assert!(parse(FileId(0), &deep).is_ok());
    }

    #[test]
    fn reports_error() {
        let e = parse(FileId(0), "machine M { start state S { entry { x = ; } } }").unwrap_err();
        assert!(e.message.contains("no viable alternative"), "{}", e.message);
    }
}
