//! Phase 5: Seam Language Parser
//!
//! Tokenizer (lexer) + recursive descent parser for the Seam language.
//! Parses Seam source code into the AST defined in seam_lang.rs.
//!
//! Supported syntax (from DRAFT spec):
//!   record Name { type field; ... }
//!   resource Name { [var] type field; ... }
//!   channel Name {
//!       resource LocalName { [var] type field; ... }
//!       requires { read { R.f; ... } write { R.f; ... } }
//!       returnType entry(argType arg, ...) { stmts }
//!       returnType collector { stmts }
//!   }

use crate::seam_lang::*;

// ===========================================================================
// Tokens
// ===========================================================================

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Declaration keywords
    KwRecord, KwResource, KwChannel,
    // Channel structure keywords
    KwEntry, KwCollector, KwRequires, KwRead, KwWrite,
    // Statement keywords
    KwReturn, KwAbort, KwFork, KwPath, KwIf, KwElse,
    // Modifier keywords
    KwVar, KwCollect, KwUnique,
    // Literal keywords
    KwTrue, KwFalse,
    // Primitive type keywords
    KwVoid, KwBool,
    KwByte, KwUByte, KwShort, KwUShort,
    KwInt, KwUInt, KwLong, KwULong,
    KwFloat, KwDouble, KwChar, KwString,
    // Punctuation
    LBrace, RBrace, LParen, RParen,
    Semicolon, Comma, Dot, Colon, Equals,
    // Values
    Ident(String),
    IntLit(i64),
    StringLit(String),
    // Sentinel
    Eof,
}

impl Token {
    fn from_keyword(s: &str) -> Option<Token> {
        match s {
            "record"    => Some(Token::KwRecord),
            "resource"  => Some(Token::KwResource),
            "channel"   => Some(Token::KwChannel),
            "entry"     => Some(Token::KwEntry),
            "collector" => Some(Token::KwCollector),
            "requires"  => Some(Token::KwRequires),
            "read"      => Some(Token::KwRead),
            "write"     => Some(Token::KwWrite),
            "return"    => Some(Token::KwReturn),
            "abort"     => Some(Token::KwAbort),
            "fork"      => Some(Token::KwFork),
            "path"      => Some(Token::KwPath),
            "if"        => Some(Token::KwIf),
            "else"      => Some(Token::KwElse),
            "var"       => Some(Token::KwVar),
            "collect"   => Some(Token::KwCollect),
            "unique"    => Some(Token::KwUnique),
            "true"      => Some(Token::KwTrue),
            "false"     => Some(Token::KwFalse),
            "void"      => Some(Token::KwVoid),
            "bool"      => Some(Token::KwBool),
            "byte"      => Some(Token::KwByte),
            "ubyte"     => Some(Token::KwUByte),
            "short"     => Some(Token::KwShort),
            "ushort"    => Some(Token::KwUShort),
            "int"       => Some(Token::KwInt),
            "uint"      => Some(Token::KwUInt),
            "long"      => Some(Token::KwLong),
            "ulong"     => Some(Token::KwULong),
            "float"     => Some(Token::KwFloat),
            "double"    => Some(Token::KwDouble),
            "char"      => Some(Token::KwChar),
            "string"    => Some(Token::KwString),
            _           => None,
        }
    }
}

// ===========================================================================
// Lexer
// ===========================================================================

pub struct Lexer<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Lexer { input, pos: 0 }
    }

    fn peek(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn peek2(&self) -> Option<char> {
        let mut it = self.input[self.pos..].chars();
        it.next();
        it.next()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            // Whitespace
            while matches!(self.peek(), Some(' ') | Some('\t') | Some('\n') | Some('\r')) {
                self.advance();
            }
            // Line comment: //
            if self.peek() == Some('/') && self.peek2() == Some('/') {
                while !matches!(self.peek(), Some('\n') | None) { self.advance(); }
                continue;
            }
            // Block comment: /* ... */
            if self.peek() == Some('/') && self.peek2() == Some('*') {
                self.advance(); self.advance();
                loop {
                    match (self.peek(), self.peek2()) {
                        (Some('*'), Some('/')) => { self.advance(); self.advance(); break; }
                        (None, _) => break,
                        _ => { self.advance(); }
                    }
                }
                continue;
            }
            break;
        }
    }

    fn read_word(&mut self, first: char) -> String {
        let mut s = String::new();
        s.push(first);
        while matches!(self.peek(), Some(c) if c.is_alphanumeric() || c == '_') {
            s.push(self.advance().unwrap());
        }
        s
    }

    fn read_number(&mut self, first: char) -> i64 {
        let mut s = String::new();
        s.push(first);
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            s.push(self.advance().unwrap());
        }
        s.parse().unwrap_or(0)
    }

    fn read_string_lit(&mut self) -> String {
        let mut s = String::new();
        loop {
            match self.peek() {
                Some('"')  => { self.advance(); break; }
                Some('\\') => {
                    self.advance();
                    match self.advance() {
                        Some('n')  => s.push('\n'),
                        Some('t')  => s.push('\t'),
                        Some('"')  => s.push('"'),
                        Some('\\') => s.push('\\'),
                        Some(c)    => { s.push('\\'); s.push(c); }
                        None => break,
                    }
                }
                Some(c) => { s.push(c); self.advance(); }
                None => break,
            }
        }
        s
    }

    /// Tokenize the entire input
    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            let tok = match self.peek() {
                None => { tokens.push(Token::Eof); break; }
                Some(c) => match c {
                    '{' => { self.advance(); Token::LBrace }
                    '}' => { self.advance(); Token::RBrace }
                    '(' => { self.advance(); Token::LParen }
                    ')' => { self.advance(); Token::RParen }
                    ';' => { self.advance(); Token::Semicolon }
                    ',' => { self.advance(); Token::Comma }
                    '.' => { self.advance(); Token::Dot }
                    ':' => { self.advance(); Token::Colon }
                    '=' => { self.advance(); Token::Equals }
                    '"' => { self.advance(); Token::StringLit(self.read_string_lit()) }
                    d if d.is_ascii_digit() => {
                        self.advance();
                        Token::IntLit(self.read_number(d))
                    }
                    c if c.is_alphabetic() || c == '_' => {
                        self.advance();
                        let word = self.read_word(c);
                        Token::from_keyword(&word).unwrap_or(Token::Ident(word))
                    }
                    _ => { self.advance(); continue; }
                }
            };
            tokens.push(tok);
        }
        tokens
    }
}

// ===========================================================================
// Parse Error
// ===========================================================================

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub token_pos: usize,
}

impl ParseError {
    fn new(msg: impl Into<String>, pos: usize) -> Self {
        ParseError { message: msg.into(), token_pos: pos }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Parse error at token {}: {}", self.token_pos, self.message)
    }
}

pub type ParseResult<T> = Result<T, ParseError>;

// ===========================================================================
// Parser
// ===========================================================================

pub struct SeamParser {
    tokens: Vec<Token>,
    pos: usize,
}

impl SeamParser {
    pub fn new(tokens: Vec<Token>) -> Self {
        SeamParser { tokens, pos: 0 }
    }

    // --- Token navigation ---

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    fn peek_next(&self) -> &Token {
        self.tokens.get(self.pos + 1).unwrap_or(&Token::Eof)
    }

    fn advance(&mut self) -> Token {
        let tok = self.tokens.get(self.pos).cloned().unwrap_or(Token::Eof);
        if self.pos < self.tokens.len() { self.pos += 1; }
        tok
    }

    fn expect_punct(&mut self, expected: &Token) -> ParseResult<()> {
        if std::mem::discriminant(self.peek()) == std::mem::discriminant(expected) {
            self.advance();
            Ok(())
        } else {
            Err(ParseError::new(format!("Expected {:?}, found {:?}", expected, self.peek()), self.pos))
        }
    }

    fn expect_ident(&mut self) -> ParseResult<String> {
        match self.advance() {
            Token::Ident(name) => Ok(name),
            tok => Err(ParseError::new(format!("Expected identifier, found {:?}", tok), self.pos))
        }
    }

    fn expect_int(&mut self) -> ParseResult<i64> {
        match self.advance() {
            Token::IntLit(n) => Ok(n),
            tok => Err(ParseError::new(format!("Expected integer literal, found {:?}", tok), self.pos))
        }
    }

    fn is_primitive_type(&self, tok: &Token) -> bool {
        matches!(tok,
            Token::KwVoid | Token::KwBool |
            Token::KwByte | Token::KwUByte | Token::KwShort | Token::KwUShort |
            Token::KwInt  | Token::KwUInt  | Token::KwLong  | Token::KwULong |
            Token::KwFloat | Token::KwDouble | Token::KwChar | Token::KwString
        )
    }

    fn is_type_start(&self, tok: &Token) -> bool {
        self.is_primitive_type(tok) || matches!(tok, Token::KwUnique | Token::Ident(_))
    }

    // --- Type parsing ---

    fn parse_type(&mut self) -> ParseResult<SeamType> {
        match self.advance() {
            Token::KwUnique => {
                let name = self.expect_ident()?;
                Ok(SeamType::Unique(name))
            }
            Token::KwVoid   => Ok(SeamType::Primitive(SeamPrimitive::Void)),
            Token::KwBool   => Ok(SeamType::Primitive(SeamPrimitive::Bool)),
            Token::KwByte   => Ok(SeamType::Primitive(SeamPrimitive::Byte)),
            Token::KwUByte  => Ok(SeamType::Primitive(SeamPrimitive::UByte)),
            Token::KwShort  => Ok(SeamType::Primitive(SeamPrimitive::Short)),
            Token::KwUShort => Ok(SeamType::Primitive(SeamPrimitive::UShort)),
            Token::KwInt    => Ok(SeamType::Primitive(SeamPrimitive::Int)),
            Token::KwUInt   => Ok(SeamType::Primitive(SeamPrimitive::UInt)),
            Token::KwLong   => Ok(SeamType::Primitive(SeamPrimitive::Long)),
            Token::KwULong  => Ok(SeamType::Primitive(SeamPrimitive::ULong)),
            Token::KwFloat  => Ok(SeamType::Primitive(SeamPrimitive::Float)),
            Token::KwDouble => Ok(SeamType::Primitive(SeamPrimitive::Double)),
            Token::KwChar   => Ok(SeamType::Primitive(SeamPrimitive::Char)),
            Token::KwString => Ok(SeamType::Primitive(SeamPrimitive::SeamString)),
            Token::Ident(n) => Ok(SeamType::Named(n)),
            tok => Err(ParseError::new(format!("Expected type, found {:?}", tok), self.pos))
        }
    }

    // --- Field parsing ---

    fn parse_field(&mut self, allow_var: bool) -> ParseResult<FieldDef> {
        let is_var = allow_var && *self.peek() == Token::KwVar && { self.advance(); true };
        let ty = self.parse_type()?;
        let name = self.expect_ident()?;
        self.expect_punct(&Token::Semicolon)?;
        Ok(FieldDef { name, ty, is_var })
    }

    // --- Top-level parsers ---

    fn parse_record(&mut self) -> ParseResult<RecordDef> {
        let name = self.expect_ident()?;
        self.expect_punct(&Token::LBrace)?;
        let mut fields = Vec::new();
        while *self.peek() != Token::RBrace && *self.peek() != Token::Eof {
            fields.push(self.parse_field(false)?);
        }
        self.expect_punct(&Token::RBrace)?;
        Ok(RecordDef { name, fields })
    }

    fn parse_resource(&mut self, is_local: bool) -> ParseResult<ResourceDef> {
        let name = self.expect_ident()?;
        self.expect_punct(&Token::LBrace)?;
        let mut fields = Vec::new();
        while *self.peek() != Token::RBrace && *self.peek() != Token::Eof {
            fields.push(self.parse_field(true)?);
        }
        self.expect_punct(&Token::RBrace)?;
        Ok(ResourceDef { name, fields, is_local })
    }

    fn parse_resource_access(&mut self) -> ParseResult<ResourceFieldAccess> {
        let resource_type = self.expect_ident()?;
        self.expect_punct(&Token::Dot)?;
        let field_name = self.expect_ident()?;
        self.expect_punct(&Token::Semicolon)?;
        Ok(ResourceFieldAccess::new(resource_type, field_name))
    }

    fn parse_requires(&mut self) -> ParseResult<RequiresBlock> {
        self.expect_punct(&Token::LBrace)?;
        let mut block = RequiresBlock::new();
        while *self.peek() != Token::RBrace && *self.peek() != Token::Eof {
            match self.peek().clone() {
                Token::KwRead => {
                    self.advance();
                    self.expect_punct(&Token::LBrace)?;
                    while *self.peek() != Token::RBrace && *self.peek() != Token::Eof {
                        block.add_read(self.parse_resource_access()?);
                    }
                    self.expect_punct(&Token::RBrace)?;
                }
                Token::KwWrite => {
                    self.advance();
                    self.expect_punct(&Token::LBrace)?;
                    while *self.peek() != Token::RBrace && *self.peek() != Token::Eof {
                        block.add_write(self.parse_resource_access()?);
                    }
                    self.expect_punct(&Token::RBrace)?;
                }
                tok => return Err(ParseError::new(
                    format!("Expected 'read' or 'write' in requires block, found {:?}", tok), self.pos
                ))
            }
        }
        self.expect_punct(&Token::RBrace)?;
        Ok(block)
    }

    // --- Expression parsing ---

    fn parse_expr(&mut self) -> ParseResult<SeamExpr> {
        match self.advance() {
            Token::IntLit(n) => Ok(SeamExpr::IntLit(n)),
            Token::StringLit(s) => Ok(SeamExpr::StringLit(s)),
            Token::KwTrue => Ok(SeamExpr::BoolLit(true)),
            Token::KwFalse => Ok(SeamExpr::BoolLit(false)),
            Token::Ident(name) => {
                if *self.peek() == Token::LParen {
                    // Call expression
                    self.advance();
                    let mut args = Vec::new();
                    while *self.peek() != Token::RParen && *self.peek() != Token::Eof {
                        args.push(self.parse_expr()?);
                        if *self.peek() == Token::Comma { self.advance(); }
                    }
                    self.expect_punct(&Token::RParen)?;
                    Ok(SeamExpr::Call { callee: name, args })
                } else if *self.peek() == Token::Dot {
                    self.advance();
                    let field = self.expect_ident()?;
                    Ok(SeamExpr::FieldAccess { expr: Box::new(SeamExpr::Ident(name)), field })
                } else {
                    Ok(SeamExpr::Ident(name))
                }
            }
            tok => Err(ParseError::new(format!("Expected expression, found {:?}", tok), self.pos))
        }
    }

    // --- Statement parsing ---

    fn parse_block(&mut self) -> ParseResult<Vec<SeamStmt>> {
        self.expect_punct(&Token::LBrace)?;
        let mut stmts = Vec::new();
        while *self.peek() != Token::RBrace && *self.peek() != Token::Eof {
            stmts.push(self.parse_stmt()?);
        }
        self.expect_punct(&Token::RBrace)?;
        Ok(stmts)
    }

    fn parse_stmt(&mut self) -> ParseResult<SeamStmt> {
        match self.peek().clone() {
            Token::KwReturn => {
                self.advance();
                if *self.peek() == Token::Semicolon {
                    self.advance();
                    Ok(SeamStmt::Return(None))
                } else {
                    let expr = self.parse_expr()?;
                    self.expect_punct(&Token::Semicolon)?;
                    Ok(SeamStmt::Return(Some(expr)))
                }
            }

            Token::KwAbort => {
                self.advance();
                self.expect_punct(&Token::Semicolon)?;
                Ok(SeamStmt::Abort)
            }

            Token::KwIf => {
                self.advance();
                self.expect_punct(&Token::LParen)?;
                let condition = self.parse_expr()?;
                self.expect_punct(&Token::RParen)?;
                let then_body = self.parse_block()?;
                let else_body = if *self.peek() == Token::KwElse {
                    self.advance();
                    Some(self.parse_block()?)
                } else {
                    None
                };
                Ok(SeamStmt::If { condition, then_body, else_body })
            }

            Token::KwFork => {
                self.advance();
                self.expect_punct(&Token::LBrace)?;
                let mut paths = Vec::new();
                while *self.peek() == Token::KwPath {
                    self.advance();
                    self.expect_punct(&Token::LParen)?;
                    let path_id = self.expect_int()? as u32;
                    self.expect_punct(&Token::RParen)?;
                    self.expect_punct(&Token::LBrace)?;
                    let requires = if *self.peek() == Token::KwRequires {
                        self.advance();
                        Some(self.parse_requires()?)
                    } else {
                        None
                    };
                    let mut body = Vec::new();
                    while *self.peek() != Token::RBrace && *self.peek() != Token::Eof {
                        body.push(self.parse_stmt()?);
                    }
                    self.expect_punct(&Token::RBrace)?;
                    paths.push(ForkPathStmt { path_id, requires, body });
                }
                self.expect_punct(&Token::RBrace)?;
                Ok(SeamStmt::Fork { paths })
            }

            // Primitive type keyword → variable declaration
            tok if self.is_primitive_type(&tok) || *self.peek() == Token::KwUnique => {
                let ty = self.parse_type()?;
                let name = self.expect_ident()?;
                let value = if *self.peek() == Token::Equals {
                    self.advance();
                    Some(self.parse_expr()?)
                } else {
                    None
                };
                self.expect_punct(&Token::Semicolon)?;
                Ok(SeamStmt::Let { name, ty, value })
            }

            // Identifier: disambiguate channel call vs named-type variable declaration
            // Lookahead: if Ident followed by '(' → call; otherwise → let binding
            Token::Ident(name) => {
                self.advance(); // consume identifier
                if *self.peek() == Token::LParen {
                    // Channel call: `Callee(args) :collect OtherChannel;`
                    self.advance(); // consume '('
                    let mut args = Vec::new();
                    while *self.peek() != Token::RParen && *self.peek() != Token::Eof {
                        args.push(self.parse_expr()?);
                        if *self.peek() == Token::Comma { self.advance(); }
                    }
                    self.expect_punct(&Token::RParen)?;
                    let collect = if *self.peek() == Token::Colon {
                        self.advance();
                        if *self.peek() == Token::KwCollect {
                            self.advance();
                            Some(self.expect_ident()?)
                        } else {
                            return Err(ParseError::new(
                                "Expected 'collect' after ':'", self.pos
                            ));
                        }
                    } else {
                        None
                    };
                    self.expect_punct(&Token::Semicolon)?;
                    Ok(SeamStmt::Call { callee: name, args, collect })
                } else {
                    // Named-type variable declaration: `TypeName varName [= expr];`
                    let ty = SeamType::Named(name);
                    let var_name = self.expect_ident()?;
                    let value = if *self.peek() == Token::Equals {
                        self.advance();
                        Some(self.parse_expr()?)
                    } else {
                        None
                    };
                    self.expect_punct(&Token::Semicolon)?;
                    Ok(SeamStmt::Let { name: var_name, ty, value })
                }
            }

            tok => Err(ParseError::new(format!("Unexpected token in statement: {:?}", tok), self.pos))
        }
    }

    // --- Parameter list parsing ---

    fn parse_params(&mut self) -> ParseResult<Vec<ParamDef>> {
        self.expect_punct(&Token::LParen)?;
        let mut params = Vec::new();
        while *self.peek() != Token::RParen && *self.peek() != Token::Eof {
            let ty = self.parse_type()?;
            let name = self.expect_ident()?;
            params.push(ParamDef::new(name, ty));
            if *self.peek() == Token::Comma { self.advance(); }
        }
        self.expect_punct(&Token::RParen)?;
        Ok(params)
    }

    // --- Channel parsing ---
    // Format:
    //   channel Name {
    //       [resource LocalName { [var] type field; ... }]
    //       [requires { read { R.f; } write { R.f; } }]
    //       returnType entry(params) { body }
    //       returnType collector { body }
    //   }

    fn parse_channel(&mut self) -> ParseResult<ChannelDef> {
        let name = self.expect_ident()?;
        self.expect_punct(&Token::LBrace)?;

        let mut local_resources = Vec::new();
        let mut requires: Option<RequiresBlock> = None;
        let mut entry: Option<EntryDef> = None;
        let mut collector: Option<CollectorDef> = None;

        while *self.peek() != Token::RBrace && *self.peek() != Token::Eof {
            match self.peek().clone() {
                Token::KwResource => {
                    self.advance();
                    local_resources.push(self.parse_resource(true)?);
                }
                Token::KwRequires => {
                    self.advance();
                    requires = Some(self.parse_requires()?);
                }
                // `returnType entry(...)` or `returnType collector`
                tok if self.is_type_start(&tok) => {
                    // Check lookahead: does Ident precede entry/collector keyword?
                    let is_named_return = matches!(tok, Token::Ident(_));
                    let next = if is_named_return {
                        self.peek_next().clone()
                    } else {
                        // For primitive types, the second token is entry/collector
                        // We parse the type first, then see what follows
                        Token::Eof // placeholder; actual check below
                    };

                    // For named types: Ident(...) followed by KwEntry or KwCollector
                    if is_named_return && !matches!(next, Token::KwEntry | Token::KwCollector) {
                        return Err(ParseError::new(
                            format!("Expected 'entry' or 'collector' after return type, found {:?}", next),
                            self.pos,
                        ));
                    }

                    let return_type = self.parse_type()?;

                    match self.peek().clone() {
                        Token::KwEntry => {
                            self.advance();
                            let params = self.parse_params()?;
                            let body = self.parse_block()?;
                            entry = Some(EntryDef { return_type, params, body });
                        }
                        Token::KwCollector => {
                            self.advance();
                            let body = self.parse_block()?;
                            collector = Some(CollectorDef { return_type, body });
                        }
                        tok => return Err(ParseError::new(
                            format!("Expected 'entry' or 'collector' after return type, found {:?}", tok),
                            self.pos,
                        ))
                    }
                }
                tok => return Err(ParseError::new(
                    format!("Unexpected token in channel body: {:?}", tok), self.pos
                ))
            }
        }

        self.expect_punct(&Token::RBrace)?;

        let entry = entry.ok_or_else(|| ParseError::new(
            format!("Channel '{}' missing entry definition", name), self.pos
        ))?;
        let collector = collector.ok_or_else(|| ParseError::new(
            format!("Channel '{}' missing collector definition", name), self.pos
        ))?;

        Ok(ChannelDef { name, local_resources, requires, entry, collector })
    }

    // --- Program entry point ---

    pub fn parse_program(&mut self) -> ParseResult<SeamProgram> {
        let mut program = SeamProgram::new();
        while *self.peek() != Token::Eof {
            match self.peek().clone() {
                Token::KwRecord => {
                    self.advance();
                    program.add_item(SeamItem::Record(self.parse_record()?));
                }
                Token::KwResource => {
                    self.advance();
                    program.add_item(SeamItem::Resource(self.parse_resource(false)?));
                }
                Token::KwChannel => {
                    self.advance();
                    program.add_item(SeamItem::Channel(self.parse_channel()?));
                }
                tok => return Err(ParseError::new(
                    format!("Expected 'record', 'resource', or 'channel', found {:?}", tok), self.pos
                ))
            }
        }
        Ok(program)
    }
}

/// Parse Seam source code into a program AST
pub fn parse_seam(source: &str) -> ParseResult<SeamProgram> {
    let tokens = Lexer::new(source).tokenize();
    SeamParser::new(tokens).parse_program()
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_primitives_and_keywords() {
        let src = "record Data { int num; string str; }";
        let tokens = Lexer::new(src).tokenize();
        assert!(tokens.contains(&Token::KwRecord));
        assert!(tokens.contains(&Token::KwInt));
        assert!(tokens.contains(&Token::KwString));
        assert!(tokens.iter().any(|t| matches!(t, Token::Ident(n) if n == "Data")));
        assert!(tokens.iter().any(|t| matches!(t, Token::Ident(n) if n == "num")));
    }

    #[test]
    fn test_parse_record() {
        let src = r#"
            record Point {
                int x;
                int y;
                string label;
            }
        "#;
        let prog = parse_seam(src).expect("Parse error");
        assert_eq!(prog.item_count(), 1);
        let rec = prog.records().next().expect("No record found");
        assert_eq!(rec.name, "Point");
        assert_eq!(rec.fields.len(), 3);
        assert_eq!(rec.fields[0].name, "x");
        assert!(!rec.fields[0].is_var);
    }

    #[test]
    fn test_parse_resource_with_var() {
        let src = r#"
            resource Counter {
                var int count;
                var string label;
            }
        "#;
        let prog = parse_seam(src).expect("Parse error");
        let res = prog.resources().next().expect("No resource");
        assert_eq!(res.name, "Counter");
        assert_eq!(res.fields.len(), 2);
        assert!(res.fields[0].is_var);
    }

    #[test]
    fn test_parse_requires_contract() {
        let src = r#"
            channel Reader {
                requires {
                    read { Counter.count; }
                    write { Counter.label; }
                }
                void entry() { return; }
                void collector { return; }
            }
        "#;
        let prog = parse_seam(src).expect("Parse error");
        let ch = prog.channels().next().expect("No channel");
        let req = ch.requires.as_ref().expect("No requires");
        assert_eq!(req.reads.len(), 1);
        assert_eq!(req.writes.len(), 1);
        assert_eq!(req.reads[0].resource_type, "Counter");
        assert_eq!(req.reads[0].field_name, "count");
    }

    #[test]
    fn test_parse_abort_and_return() {
        let src = r#"
            channel ErrorPath {
                void entry() { abort; }
                void collector { return; }
            }
        "#;
        let prog = parse_seam(src).expect("Parse error");
        let ch = prog.channels().next().expect("No channel");
        assert!(matches!(ch.entry.body[0], SeamStmt::Abort));
        assert!(matches!(ch.collector.body[0], SeamStmt::Return(None)));
    }

    #[test]
    fn test_parse_collect_binding() {
        let src = r#"
            channel Parent {
                void entry() {
                    Child() :collect GrandChild;
                    return;
                }
                void collector { return; }
            }
        "#;
        let prog = parse_seam(src).expect("Parse error");
        let ch = prog.channels().next().expect("No channel");
        match &ch.entry.body[0] {
            SeamStmt::Call { callee, collect, args } => {
                assert_eq!(callee, "Child");
                assert_eq!(collect.as_deref(), Some("GrandChild"));
                assert!(args.is_empty());
            }
            _ => panic!("Expected Call statement"),
        }
    }

    #[test]
    fn test_parse_fork_statement() {
        let src = r#"
            channel Concurrent {
                void entry() {
                    fork {
                        path(0) {
                            requires { read { State.value; } }
                            return;
                        }
                        path(1) {
                            requires { write { State.value; } }
                            return;
                        }
                    }
                }
                void collector { return; }
            }
        "#;
        let prog = parse_seam(src).expect("Parse error");
        let ch = prog.channels().next().expect("No channel");
        match &ch.entry.body[0] {
            SeamStmt::Fork { paths } => {
                assert_eq!(paths.len(), 2);
                assert_eq!(paths[0].path_id, 0);
                assert_eq!(paths[1].path_id, 1);
                assert!(paths[0].requires.as_ref().unwrap().reads.len() == 1);
                assert!(paths[1].requires.as_ref().unwrap().writes.len() == 1);
            }
            _ => panic!("Expected Fork statement"),
        }
    }

    #[test]
    fn test_parse_multiple_items_with_comments() {
        let src = r#"
            // Data model
            record Point { int x; int y; }

            /* Shared state */
            resource State { int count; }

            channel Handler {
                void entry() { return; }
                void collector { return; }
            }
        "#;
        let prog = parse_seam(src).expect("Parse error");
        assert_eq!(prog.item_count(), 3);
        assert_eq!(prog.records().count(), 1);
        assert_eq!(prog.resources().count(), 1);
        assert_eq!(prog.channels().count(), 1);
    }

    #[test]
    fn test_parse_if_statement() {
        let src = r#"
            channel Conditional {
                void entry(int x) {
                    if (x) {
                        return;
                    } else {
                        abort;
                    }
                }
                void collector { return; }
            }
        "#;
        let prog = parse_seam(src).expect("Parse error");
        let ch = prog.channels().next().expect("No channel");
        assert!(matches!(ch.entry.body[0], SeamStmt::If { .. }));
    }
}
