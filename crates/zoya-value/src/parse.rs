use std::collections::{HashMap, HashSet};

use logos::Logos;
use zoya_ir::{DefinitionLookup, EnumVariantType, QualifiedPath, Type};

use crate::{Error, Value, ValueData, variant_type_to_fields};

// ── Tokens ────────────────────────────────────────────────────────────

fn parse_string(lex: &logos::Lexer<Token>) -> Option<String> {
    let s = lex.slice();
    let inner = &s[1..s.len() - 1];
    let mut result = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next()? {
                'n' => result.push('\n'),
                't' => result.push('\t'),
                '\\' => result.push('\\'),
                '"' => result.push('"'),
                other => {
                    result.push('\\');
                    result.push(other);
                }
            }
        } else {
            result.push(c);
        }
    }
    Some(result)
}

fn parse_float(lex: &logos::Lexer<Token>) -> Option<f64> {
    lex.slice().parse().ok()
}

fn parse_neg_float(lex: &logos::Lexer<Token>) -> Option<f64> {
    lex.slice().parse().ok()
}

fn parse_int(lex: &logos::Lexer<Token>) -> Option<i64> {
    lex.slice().parse().ok()
}

fn parse_neg_int(lex: &logos::Lexer<Token>) -> Option<i64> {
    lex.slice().parse().ok()
}

#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t\n\r]+")]
enum Token {
    #[token("true")]
    True,

    #[token("false")]
    False,

    #[regex(r#""([^"\\]|\\.)*""#, parse_string)]
    String(String),

    // Float must come before Int so "3.14" matches as float, not "3" + error
    #[regex(r"-[0-9]+\.[0-9]+", parse_neg_float)]
    NegFloat(f64),

    #[regex(r"[0-9]+\.[0-9]+", parse_float)]
    Float(f64),

    #[regex(r"-[0-9]+", parse_neg_int)]
    NegInt(i64),

    #[regex(r"[0-9]+", parse_int)]
    Int(i64),

    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*", |lex| lex.slice().to_string())]
    Ident(String),

    #[token("(")]
    LParen,

    #[token(")")]
    RParen,

    #[token("[")]
    LBracket,

    #[token("]")]
    RBracket,

    #[token("{")]
    LBrace,

    #[token("}")]
    RBrace,

    #[token(",")]
    Comma,

    #[token("::")]
    ColonColon,

    #[token(":")]
    Colon,

    Eof,
}

impl Token {
    fn describe(&self) -> &'static str {
        match self {
            Token::Int(_) | Token::NegInt(_) => "integer",
            Token::Float(_) | Token::NegFloat(_) => "float",
            Token::True | Token::False => "boolean",
            Token::String(_) => "string",
            Token::Ident(_) => "identifier",
            Token::LParen => "'('",
            Token::RParen => "')'",
            Token::LBracket => "'['",
            Token::RBracket => "']'",
            Token::LBrace => "'{'",
            Token::RBrace => "'}'",
            Token::Comma => "','",
            Token::Colon => "':'",
            Token::ColonColon => "'::'",
            Token::Eof => "end of input",
        }
    }
}

// ── Tokenizer ─────────────────────────────────────────────────────────

fn tokenize(input: &str) -> Result<Vec<Token>, Error> {
    let mut tokens = Vec::new();
    let mut lexer = Token::lexer(input);

    while let Some(result) = lexer.next() {
        match result {
            Ok(token) => tokens.push(token),
            Err(()) => {
                return Err(Error::ParseError(format!(
                    "unexpected character: '{}'",
                    lexer.slice()
                )));
            }
        }
    }

    tokens.push(Token::Eof);
    Ok(tokens)
}

// ── Parser ────────────────────────────────────────────────────────────

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn advance(&mut self) -> Token {
        let tok = self.tokens[self.pos].clone();
        self.pos += 1;
        tok
    }

    fn expect_eof(&self) -> Result<(), Error> {
        if !matches!(self.peek(), Token::Eof) {
            return Err(Error::ParseError(format!(
                "unexpected trailing input: {}",
                self.peek().describe()
            )));
        }
        Ok(())
    }

    fn parse(&mut self, expected: &Type, type_lookup: &DefinitionLookup) -> Result<Value, Error> {
        match expected {
            Type::String => self.parse_string(),
            Type::Int => self.parse_int(),
            Type::BigInt => self.parse_bigint(),
            Type::Float => self.parse_float(),
            Type::Bool => self.parse_bool(),
            Type::List(elem) => self.parse_list(elem, type_lookup),
            Type::Tuple(types) => self.parse_tuple(types, type_lookup),
            Type::Set(elem) => self.parse_set(elem, type_lookup),
            Type::Dict(key, val) => self.parse_dict(key, val, type_lookup),
            Type::Struct {
                module,
                name,
                type_args,
                fields,
            } => self.parse_struct(module, name, type_args, fields, type_lookup),
            Type::Enum {
                module,
                name,
                type_args,
                variants,
            } => self.parse_enum(module, name, type_args, variants, type_lookup),
            Type::Var(_) => Err(Error::ParseError("type variables are not supported".into())),
            Type::Function { .. } => {
                Err(Error::ParseError("function types are not supported".into()))
            }
        }
    }

    fn parse_int(&mut self) -> Result<Value, Error> {
        match self.advance() {
            Token::Int(n) | Token::NegInt(n) => Ok(Value::Int(n)),
            tok => Err(Error::ParseError(format!(
                "expected Int, got {}",
                tok.describe()
            ))),
        }
    }

    fn parse_bigint(&mut self) -> Result<Value, Error> {
        match self.advance() {
            Token::Int(n) | Token::NegInt(n) => Ok(Value::BigInt(n)),
            tok => Err(Error::ParseError(format!(
                "expected BigInt, got {}",
                tok.describe()
            ))),
        }
    }

    fn parse_float(&mut self) -> Result<Value, Error> {
        match self.advance() {
            Token::Float(f) | Token::NegFloat(f) => Ok(Value::Float(f)),
            Token::Int(n) | Token::NegInt(n) => Ok(Value::Float(n as f64)),
            tok => Err(Error::ParseError(format!(
                "expected Float, got {}",
                tok.describe()
            ))),
        }
    }

    fn parse_bool(&mut self) -> Result<Value, Error> {
        match self.advance() {
            Token::True => Ok(Value::Bool(true)),
            Token::False => Ok(Value::Bool(false)),
            tok => Err(Error::ParseError(format!(
                "expected Bool, got {}",
                tok.describe()
            ))),
        }
    }

    fn parse_string(&mut self) -> Result<Value, Error> {
        match self.advance() {
            Token::String(s) => Ok(Value::String(s)),
            tok => Err(Error::ParseError(format!(
                "expected quoted string, got {}",
                tok.describe()
            ))),
        }
    }

    fn parse_list(
        &mut self,
        elem_type: &Type,
        type_lookup: &DefinitionLookup,
    ) -> Result<Value, Error> {
        self.expect_token(Token::LBracket)?;
        let mut items = Vec::new();
        while !matches!(self.peek(), Token::RBracket) {
            items.push(self.parse(elem_type, type_lookup)?);
            if matches!(self.peek(), Token::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        self.expect_token(Token::RBracket)?;
        Ok(Value::List(items))
    }

    fn parse_tuple(
        &mut self,
        types: &[Type],
        type_lookup: &DefinitionLookup,
    ) -> Result<Value, Error> {
        self.expect_token(Token::LParen)?;
        let mut items = Vec::new();
        for (i, ty) in types.iter().enumerate() {
            if i > 0 {
                self.expect_token(Token::Comma)?;
            }
            items.push(self.parse(ty, type_lookup)?);
        }
        // Allow trailing comma
        if matches!(self.peek(), Token::Comma) {
            self.advance();
        }
        self.expect_token(Token::RParen)?;
        Ok(Value::Tuple(items))
    }

    fn parse_set(
        &mut self,
        elem_type: &Type,
        type_lookup: &DefinitionLookup,
    ) -> Result<Value, Error> {
        self.expect_token(Token::LBrace)?;
        let mut items = HashSet::new();
        while !matches!(self.peek(), Token::RBrace) {
            items.insert(self.parse(elem_type, type_lookup)?);
            if matches!(self.peek(), Token::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        self.expect_token(Token::RBrace)?;
        Ok(Value::Set(items))
    }

    fn parse_dict(
        &mut self,
        key_type: &Type,
        val_type: &Type,
        type_lookup: &DefinitionLookup,
    ) -> Result<Value, Error> {
        self.expect_token(Token::LBrace)?;
        let mut entries = HashMap::new();
        while !matches!(self.peek(), Token::RBrace) {
            let key = self.parse(key_type, type_lookup)?;
            self.expect_token(Token::Colon)?;
            let val = self.parse(val_type, type_lookup)?;
            entries.insert(key, val);
            if matches!(self.peek(), Token::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        self.expect_token(Token::RBrace)?;
        Ok(Value::Dict(entries))
    }

    fn parse_struct(
        &mut self,
        module: &QualifiedPath,
        name: &str,
        type_args: &[Type],
        fields: &[(String, Type)],
        type_lookup: &DefinitionLookup,
    ) -> Result<Value, Error> {
        let resolved_fields = type_lookup.resolve_struct_fields(module, name, fields, type_args);

        // Expect the struct name
        match self.advance() {
            Token::Ident(ref ident) if ident == name => {}
            tok => {
                return Err(Error::ParseError(format!(
                    "expected struct name '{name}', got {}",
                    tok.describe()
                )));
            }
        }

        let data = if resolved_fields.is_empty() {
            // Unit struct
            ValueData::Unit
        } else if resolved_fields[0].0.starts_with('$') {
            // Tuple struct: Name(val, val, ...)
            self.parse_tuple_data(&resolved_fields, type_lookup)?
        } else {
            // Named struct: Name { field: val, ... }
            self.parse_struct_data(&resolved_fields, type_lookup)?
        };

        Ok(Value::Struct {
            name: name.to_string(),
            module: module.clone(),
            data,
        })
    }

    fn parse_enum(
        &mut self,
        module: &QualifiedPath,
        enum_name: &str,
        type_args: &[Type],
        variants: &[(String, EnumVariantType)],
        type_lookup: &DefinitionLookup,
    ) -> Result<Value, Error> {
        let resolved_variants =
            type_lookup.resolve_enum_variants(module, enum_name, variants, type_args);

        // Parse variant name — supports both `Variant` and `Enum::Variant`
        let variant_name = match self.advance() {
            Token::Ident(ident) => {
                if matches!(self.peek(), Token::ColonColon) {
                    // Could be Enum::Variant form
                    if ident == enum_name {
                        self.advance(); // consume ::
                        match self.advance() {
                            Token::Ident(vname) => vname,
                            tok => {
                                return Err(Error::ParseError(format!(
                                    "expected variant name after '{enum_name}::', got {}",
                                    tok.describe()
                                )));
                            }
                        }
                    } else {
                        // Not the enum name — treat as a plain variant name
                        ident
                    }
                } else {
                    ident
                }
            }
            tok => {
                return Err(Error::ParseError(format!(
                    "expected variant name, got {}",
                    tok.describe()
                )));
            }
        };

        let variant_type = resolved_variants
            .iter()
            .find(|(vname, _)| vname == &variant_name)
            .map(|(_, vt)| vt)
            .ok_or_else(|| {
                Error::ParseError(format!(
                    "unknown variant '{variant_name}' for enum '{enum_name}'"
                ))
            })?;

        let variant_fields = variant_type_to_fields(variant_type);

        let data = if variant_fields.is_empty() {
            ValueData::Unit
        } else if variant_fields[0].0.starts_with('$') {
            self.parse_tuple_data(&variant_fields, type_lookup)?
        } else {
            self.parse_struct_data(&variant_fields, type_lookup)?
        };

        Ok(Value::EnumVariant {
            enum_name: enum_name.to_string(),
            variant_name,
            module: module.clone(),
            data,
        })
    }

    /// Parse `(val, val, ...)` for tuple struct/enum fields.
    fn parse_tuple_data(
        &mut self,
        fields: &[(String, Type)],
        type_lookup: &DefinitionLookup,
    ) -> Result<ValueData, Error> {
        self.expect_token(Token::LParen)?;
        let mut values = Vec::new();
        for (i, (_, ty)) in fields.iter().enumerate() {
            if i > 0 {
                self.expect_token(Token::Comma)?;
            }
            values.push(self.parse(ty, type_lookup)?);
        }
        // Allow trailing comma
        if matches!(self.peek(), Token::Comma) {
            self.advance();
        }
        self.expect_token(Token::RParen)?;
        Ok(ValueData::Tuple(values))
    }

    /// Parse `{ field: val, field: val, ... }` for named struct/enum fields.
    fn parse_struct_data(
        &mut self,
        fields: &[(String, Type)],
        type_lookup: &DefinitionLookup,
    ) -> Result<ValueData, Error> {
        self.expect_token(Token::LBrace)?;

        let field_map: HashMap<&str, &Type> = fields.iter().map(|(n, t)| (n.as_str(), t)).collect();
        let mut values = HashMap::new();

        while !matches!(self.peek(), Token::RBrace) {
            let field_name = match self.advance() {
                Token::Ident(name) => name,
                tok => {
                    return Err(Error::ParseError(format!(
                        "expected field name, got {}",
                        tok.describe()
                    )));
                }
            };
            let field_type = field_map
                .get(field_name.as_str())
                .ok_or_else(|| Error::ParseError(format!("unknown field: '{field_name}'")))?;
            self.expect_token(Token::Colon)?;
            let val = self.parse(field_type, type_lookup)?;
            values.insert(field_name, val);

            if matches!(self.peek(), Token::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        self.expect_token(Token::RBrace)?;
        Ok(ValueData::Struct(values))
    }

    fn expect_token(&mut self, expected: Token) -> Result<(), Error> {
        let tok = self.advance();
        if std::mem::discriminant(&tok) == std::mem::discriminant(&expected) {
            Ok(())
        } else {
            Err(Error::ParseError(format!(
                "expected {}, got {}",
                expected.describe(),
                tok.describe()
            )))
        }
    }
}

// ── Public entry point ────────────────────────────────────────────────

pub(crate) fn parse_value(
    input: &str,
    expected: &Type,
    type_lookup: &DefinitionLookup,
) -> Result<Value, Error> {
    // String special case: return raw input, no tokenization
    if matches!(expected, Type::String) {
        return Ok(Value::String(input.to_string()));
    }

    let tokens = tokenize(input)?;
    let mut parser = Parser::new(tokens);
    let value = parser.parse(expected, type_lookup)?;
    parser.expect_eof()?;
    Ok(value)
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use zoya_ir::DefinitionLookup;

    fn empty_lookup() -> DefinitionLookup {
        DefinitionLookup::empty()
    }

    // ── Scalar types ──────────────────────────────────────────────

    #[test]
    fn parse_int() {
        let lookup = empty_lookup();
        assert_eq!(
            parse_value("42", &Type::Int, &lookup).unwrap(),
            Value::Int(42)
        );
    }

    #[test]
    fn parse_negative_int() {
        let lookup = empty_lookup();
        assert_eq!(
            parse_value("-7", &Type::Int, &lookup).unwrap(),
            Value::Int(-7)
        );
    }

    #[test]
    fn parse_zero_int() {
        let lookup = empty_lookup();
        assert_eq!(
            parse_value("0", &Type::Int, &lookup).unwrap(),
            Value::Int(0)
        );
    }

    #[test]
    fn parse_bigint() {
        let lookup = empty_lookup();
        assert_eq!(
            parse_value("99", &Type::BigInt, &lookup).unwrap(),
            Value::BigInt(99)
        );
    }

    #[test]
    fn parse_float() {
        let lookup = empty_lookup();
        assert_eq!(
            parse_value("3.14", &Type::Float, &lookup).unwrap(),
            Value::Float(3.14)
        );
    }

    #[test]
    fn parse_negative_float() {
        let lookup = empty_lookup();
        assert_eq!(
            parse_value("-2.5", &Type::Float, &lookup).unwrap(),
            Value::Float(-2.5)
        );
    }

    #[test]
    fn parse_int_as_float() {
        let lookup = empty_lookup();
        assert_eq!(
            parse_value("42", &Type::Float, &lookup).unwrap(),
            Value::Float(42.0)
        );
    }

    #[test]
    fn parse_bool_true() {
        let lookup = empty_lookup();
        assert_eq!(
            parse_value("true", &Type::Bool, &lookup).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn parse_bool_false() {
        let lookup = empty_lookup();
        assert_eq!(
            parse_value("false", &Type::Bool, &lookup).unwrap(),
            Value::Bool(false)
        );
    }

    // ── String raw passthrough ────────────────────────────────────

    #[test]
    fn parse_string_raw() {
        let lookup = empty_lookup();
        assert_eq!(
            parse_value("hello world", &Type::String, &lookup).unwrap(),
            Value::String("hello world".into())
        );
    }

    #[test]
    fn parse_string_that_looks_numeric() {
        let lookup = empty_lookup();
        assert_eq!(
            parse_value("42", &Type::String, &lookup).unwrap(),
            Value::String("42".into())
        );
    }

    #[test]
    fn parse_string_empty() {
        let lookup = empty_lookup();
        assert_eq!(
            parse_value("", &Type::String, &lookup).unwrap(),
            Value::String("".into())
        );
    }

    // ── Lists ─────────────────────────────────────────────────────

    #[test]
    fn parse_list_of_ints() {
        let lookup = empty_lookup();
        assert_eq!(
            parse_value("[1, 2, 3]", &Type::List(Box::new(Type::Int)), &lookup).unwrap(),
            Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
        );
    }

    #[test]
    fn parse_list_empty() {
        let lookup = empty_lookup();
        assert_eq!(
            parse_value("[]", &Type::List(Box::new(Type::Int)), &lookup).unwrap(),
            Value::List(vec![])
        );
    }

    #[test]
    fn parse_list_nested() {
        let lookup = empty_lookup();
        let ty = Type::List(Box::new(Type::List(Box::new(Type::Int))));
        assert_eq!(
            parse_value("[[1, 2], [3]]", &ty, &lookup).unwrap(),
            Value::List(vec![
                Value::List(vec![Value::Int(1), Value::Int(2)]),
                Value::List(vec![Value::Int(3)]),
            ])
        );
    }

    #[test]
    fn parse_list_trailing_comma() {
        let lookup = empty_lookup();
        assert_eq!(
            parse_value("[1, 2,]", &Type::List(Box::new(Type::Int)), &lookup).unwrap(),
            Value::List(vec![Value::Int(1), Value::Int(2)])
        );
    }

    #[test]
    fn parse_list_with_spaces() {
        let lookup = empty_lookup();
        assert_eq!(
            parse_value("[ 1 , 2 , 3 ]", &Type::List(Box::new(Type::Int)), &lookup).unwrap(),
            Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
        );
    }

    // ── Tuples ────────────────────────────────────────────────────

    #[test]
    fn parse_tuple() {
        let lookup = empty_lookup();
        let ty = Type::Tuple(vec![Type::Int, Type::Bool]);
        assert_eq!(
            parse_value("(42, true)", &ty, &lookup).unwrap(),
            Value::Tuple(vec![Value::Int(42), Value::Bool(true)])
        );
    }

    #[test]
    fn parse_tuple_single() {
        let lookup = empty_lookup();
        let ty = Type::Tuple(vec![Type::Int]);
        assert_eq!(
            parse_value("(1,)", &ty, &lookup).unwrap(),
            Value::Tuple(vec![Value::Int(1)])
        );
    }

    #[test]
    fn parse_tuple_empty() {
        let lookup = empty_lookup();
        let ty = Type::Tuple(vec![]);
        assert_eq!(
            parse_value("()", &ty, &lookup).unwrap(),
            Value::Tuple(vec![])
        );
    }

    // ── Sets ──────────────────────────────────────────────────────

    #[test]
    fn parse_set_of_ints() {
        let lookup = empty_lookup();
        let result = parse_value("{1, 2, 3}", &Type::Set(Box::new(Type::Int)), &lookup).unwrap();
        let mut expected = HashSet::new();
        expected.insert(Value::Int(1));
        expected.insert(Value::Int(2));
        expected.insert(Value::Int(3));
        assert_eq!(result, Value::Set(expected));
    }

    // ── Dicts ─────────────────────────────────────────────────────

    #[test]
    fn parse_dict() {
        let lookup = empty_lookup();
        let ty = Type::Dict(Box::new(Type::String), Box::new(Type::Int));
        let result = parse_value(r#"{"a": 1, "b": 2}"#, &ty, &lookup).unwrap();
        let mut expected = HashMap::new();
        expected.insert(Value::String("a".into()), Value::Int(1));
        expected.insert(Value::String("b".into()), Value::Int(2));
        assert_eq!(result, Value::Dict(expected));
    }

    // ── Structs ───────────────────────────────────────────────────

    fn root() -> QualifiedPath {
        QualifiedPath::new(vec!["root".into()])
    }

    #[test]
    fn parse_unit_struct() {
        let lookup = empty_lookup();
        let ty = Type::Struct {
            module: root(),
            name: "Empty".into(),
            type_args: vec![],
            fields: vec![],
        };
        assert_eq!(
            parse_value("Empty", &ty, &lookup).unwrap(),
            Value::Struct {
                name: "Empty".into(),
                module: root(),
                data: ValueData::Unit,
            }
        );
    }

    #[test]
    fn parse_tuple_struct() {
        let lookup = empty_lookup();
        let ty = Type::Struct {
            module: root(),
            name: "Pair".into(),
            type_args: vec![],
            fields: vec![("$0".into(), Type::Int), ("$1".into(), Type::Bool)],
        };
        assert_eq!(
            parse_value("Pair(1, true)", &ty, &lookup).unwrap(),
            Value::Struct {
                name: "Pair".into(),
                module: root(),
                data: ValueData::Tuple(vec![Value::Int(1), Value::Bool(true)]),
            }
        );
    }

    #[test]
    fn parse_named_struct() {
        let lookup = empty_lookup();
        let ty = Type::Struct {
            module: root(),
            name: "Point".into(),
            type_args: vec![],
            fields: vec![("x".into(), Type::Int), ("y".into(), Type::Int)],
        };
        let result = parse_value("Point{x: 1, y: 2}", &ty, &lookup).unwrap();
        let mut fields = HashMap::new();
        fields.insert("x".into(), Value::Int(1));
        fields.insert("y".into(), Value::Int(2));
        assert_eq!(
            result,
            Value::Struct {
                name: "Point".into(),
                module: root(),
                data: ValueData::Struct(fields),
            }
        );
    }

    // ── Enums ─────────────────────────────────────────────────────

    #[test]
    fn parse_unit_variant() {
        let lookup = empty_lookup();
        let ty = Type::Enum {
            module: root(),
            name: "Color".into(),
            type_args: vec![],
            variants: vec![
                ("Red".into(), EnumVariantType::Unit),
                ("Green".into(), EnumVariantType::Unit),
            ],
        };
        assert_eq!(
            parse_value("Red", &ty, &lookup).unwrap(),
            Value::EnumVariant {
                enum_name: "Color".into(),
                variant_name: "Red".into(),
                module: root(),
                data: ValueData::Unit,
            }
        );
    }

    #[test]
    fn parse_tuple_variant() {
        let lookup = empty_lookup();
        let ty = Type::Enum {
            module: root(),
            name: "Option".into(),
            type_args: vec![],
            variants: vec![
                ("Some".into(), EnumVariantType::Tuple(vec![Type::Int])),
                ("None".into(), EnumVariantType::Unit),
            ],
        };
        assert_eq!(
            parse_value("Some(42)", &ty, &lookup).unwrap(),
            Value::EnumVariant {
                enum_name: "Option".into(),
                variant_name: "Some".into(),
                module: root(),
                data: ValueData::Tuple(vec![Value::Int(42)]),
            }
        );
    }

    #[test]
    fn parse_struct_variant() {
        let lookup = empty_lookup();
        let ty = Type::Enum {
            module: root(),
            name: "Shape".into(),
            type_args: vec![],
            variants: vec![(
                "Rect".into(),
                EnumVariantType::Struct(vec![("w".into(), Type::Int), ("h".into(), Type::Int)]),
            )],
        };
        let result = parse_value("Rect{w: 3, h: 4}", &ty, &lookup).unwrap();
        let mut fields = HashMap::new();
        fields.insert("w".into(), Value::Int(3));
        fields.insert("h".into(), Value::Int(4));
        assert_eq!(
            result,
            Value::EnumVariant {
                enum_name: "Shape".into(),
                variant_name: "Rect".into(),
                module: root(),
                data: ValueData::Struct(fields),
            }
        );
    }

    #[test]
    fn parse_qualified_enum_variant() {
        let lookup = empty_lookup();
        let ty = Type::Enum {
            module: root(),
            name: "Option".into(),
            type_args: vec![],
            variants: vec![
                ("Some".into(), EnumVariantType::Tuple(vec![Type::Int])),
                ("None".into(), EnumVariantType::Unit),
            ],
        };
        assert_eq!(
            parse_value("Option::Some(7)", &ty, &lookup).unwrap(),
            Value::EnumVariant {
                enum_name: "Option".into(),
                variant_name: "Some".into(),
                module: root(),
                data: ValueData::Tuple(vec![Value::Int(7)]),
            }
        );
    }

    #[test]
    fn parse_qualified_unit_variant() {
        let lookup = empty_lookup();
        let ty = Type::Enum {
            module: root(),
            name: "Option".into(),
            type_args: vec![],
            variants: vec![
                ("Some".into(), EnumVariantType::Tuple(vec![Type::Int])),
                ("None".into(), EnumVariantType::Unit),
            ],
        };
        assert_eq!(
            parse_value("Option::None", &ty, &lookup).unwrap(),
            Value::EnumVariant {
                enum_name: "Option".into(),
                variant_name: "None".into(),
                module: root(),
                data: ValueData::Unit,
            }
        );
    }

    // ── Strings inside compound types ─────────────────────────────

    #[test]
    fn parse_list_of_strings() {
        let lookup = empty_lookup();
        let ty = Type::List(Box::new(Type::String));
        assert_eq!(
            parse_value(r#"["hello", "world"]"#, &ty, &lookup).unwrap(),
            Value::List(vec![
                Value::String("hello".into()),
                Value::String("world".into()),
            ])
        );
    }

    // ── Error cases ───────────────────────────────────────────────

    #[test]
    fn parse_int_invalid() {
        let lookup = empty_lookup();
        assert!(parse_value("abc", &Type::Int, &lookup).is_err());
    }

    #[test]
    fn parse_bool_invalid() {
        let lookup = empty_lookup();
        assert!(parse_value("42", &Type::Bool, &lookup).is_err());
    }

    #[test]
    fn parse_trailing_input() {
        let lookup = empty_lookup();
        assert!(parse_value("42 extra", &Type::Int, &lookup).is_err());
    }

    #[test]
    fn parse_unknown_variant() {
        let lookup = empty_lookup();
        let ty = Type::Enum {
            module: root(),
            name: "Color".into(),
            type_args: vec![],
            variants: vec![("Red".into(), EnumVariantType::Unit)],
        };
        assert!(parse_value("Blue", &ty, &lookup).is_err());
    }

    #[test]
    fn parse_unsupported_type_var() {
        let lookup = empty_lookup();
        assert!(parse_value("42", &Type::Var(zoya_ir::TypeVarId(0)), &lookup).is_err());
    }

    #[test]
    fn parse_unsupported_function() {
        let lookup = empty_lookup();
        let ty = Type::Function {
            params: vec![Type::Int],
            ret: Box::new(Type::Int),
        };
        assert!(parse_value("42", &ty, &lookup).is_err());
    }

    #[test]
    fn parse_unterminated_string() {
        let lookup = empty_lookup();
        let ty = Type::List(Box::new(Type::String));
        assert!(parse_value(r#"["hello]"#, &ty, &lookup).is_err());
    }

    #[test]
    fn parse_unexpected_char() {
        let lookup = empty_lookup();
        assert!(parse_value("@", &Type::Int, &lookup).is_err());
    }

    #[test]
    fn parse_string_with_escapes() {
        let lookup = empty_lookup();
        let ty = Type::List(Box::new(Type::String));
        assert_eq!(
            parse_value(r#"["hello\nworld"]"#, &ty, &lookup).unwrap(),
            Value::List(vec![Value::String("hello\nworld".into())])
        );
    }
}
