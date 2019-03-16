use std::collections::HashMap;
use std::fmt;

use lazy_static::lazy_static;

use crate::ast;
use crate::lexer;
use crate::token;
use trace::trace;

trace::init_depth_var!();

// TODO: Consider using Pratt terminology like `nuds` and `leds`
// Not sure it's very comprehensible, though...

// Priority table for different fixity operations
lazy_static! {
    static ref PRECEDENCES: HashMap<token::TokenType, u8> = [
        (token::EQ.to_string(), token::EQUALS),
        (token::NOT_EQ.to_string(), token::EQUALS),
        (token::LT.to_string(), token::LESSGREATER),
        (token::GT.to_string(), token::LESSGREATER),
        (token::PLUS.to_string(), token::SUM),
        (token::MINUS.to_string(), token::SUM),
        (token::SLASH.to_string(), token::PRODUCT),
        (token::ASTERISK.to_string(), token::PRODUCT),
    ]
    .iter()
    .cloned()
    .collect();
}

// TODO: type alias for precedence instead of u8?
fn precedence_by_token_type(token_type: &token::TokenType) -> u8 {
    match PRECEDENCES.get(token_type) {
        Some(precedence) => *precedence,
        None => token::LOWEST,
    }
}

// For debug visualization I migth potentially use this approach
// https://users.rust-lang.org/t/is-it-possible-to-implement-debug-for-fn-type/14824

// Greeting to the master of functinal Rust - mighty @raventid
type PrefixParseFnAlias = Fn(&mut Parser) -> token::Expression + 'static;

struct PrefixParseFn(Box<PrefixParseFnAlias>);
impl fmt::Debug for PrefixParseFn {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", "prefix_parse_fn")
    }
}

type InfixParseFnAlias = Fn(&mut Parser, token::Expression) -> token::Expression + 'static;

struct InfixParseFn(Box<InfixParseFnAlias>);
impl fmt::Debug for InfixParseFn {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", "infix_parse_fn")
    }
}

#[derive(Debug)]
struct LambdaParsers {
    pub prefix_parse_fns: HashMap<token::TokenType, PrefixParseFn>,
    pub infix_parse_fns: HashMap<token::TokenType, InfixParseFn>,
}

impl LambdaParsers {
    fn register_parsers(&mut self) {
        // PREFIX PARSERS
        self.register_prefix(
            token::IDENT.to_string(),
            Box::new(|parser| {
                token::Expression::Identifier(token::Identifier {
                    token: parser.current_token.clone(),
                    value: parser.current_token.literal.clone(),
                })
            }),
        );

        self.register_prefix(token::INT.to_string(), Box::new(Self::parse_int_literal));

        self.register_prefix(
            token::BANG.to_string(),
            Box::new(Self::parse_prefix_expression),
        );

        self.register_prefix(
            token::MINUS.to_string(),
            Box::new(Self::parse_prefix_expression),
        );

        self.register_prefix(token::TRUE.to_string(), Box::new(Self::parse_boolean));

        self.register_prefix(token::FALSE.to_string(), Box::new(Self::parse_boolean));

        self.register_prefix(
            token::LPAREN.to_string(),
            Box::new(Self::parse_grouped_expressions),
        );

        // INFIX PARSERS
        self.register_infix(
            token::PLUS.to_string(),
            Box::new(Self::parse_infix_expression),
        );

        self.register_infix(
            token::MINUS.to_string(),
            Box::new(Self::parse_infix_expression),
        );

        self.register_infix(
            token::SLASH.to_string(),
            Box::new(Self::parse_infix_expression),
        );

        self.register_infix(
            token::ASTERISK.to_string(),
            Box::new(Self::parse_infix_expression),
        );

        self.register_infix(
            token::EQ.to_string(),
            Box::new(Self::parse_infix_expression),
        );

        self.register_infix(
            token::NOT_EQ.to_string(),
            Box::new(Self::parse_infix_expression),
        );

        self.register_infix(
            token::LT.to_string(),
            Box::new(Self::parse_infix_expression),
        );

        self.register_infix(
            token::GT.to_string(),
            Box::new(Self::parse_infix_expression),
        );
    }

    fn register_prefix(&mut self, token_type: token::TokenType, f: Box<PrefixParseFnAlias>) {
        self.prefix_parse_fns.insert(token_type, PrefixParseFn(f));
    }

    fn register_infix(&mut self, token_type: token::TokenType, f: Box<InfixParseFnAlias>) {
        self.infix_parse_fns.insert(token_type, InfixParseFn(f));
    }

    fn parse_identifier(parser: &mut Parser) -> token::Expression {
        token::Expression::Identifier(token::Identifier {
            token: parser.current_token.clone(),
            value: parser.current_token.literal.clone(),
        })
    }

    fn parse_int_literal(parser: &mut Parser) -> token::Expression {
        let to_be_integer = parser.current_token.literal.clone();

        // TODO: This extremly bad
        // Lambda parsers should bubble errors to parser.
        // Parser should handle them gracefully.
        // For the future:
        //
        // enum ParserError {
        //     FailedToReconiseIntegerLiteral(parse_int_error),
        //     FailedToObtainSomeValue(some_error_message),
        // }
        let integer = to_be_integer.parse::<i32>().unwrap();

        token::Expression::IntegerLiteral(token::IntegerLiteral {
            token: parser.current_token.clone(),
            value: integer,
        })
    }

    fn parse_boolean(parser: &mut Parser) -> token::Expression {
        // TODO: extract?
        // fn (parser: &Parser) cur_token_is(t: token::TokenType) -> bool { p.cur_token.type == t }
        let boolean_value = parser.current_token.token_type == token::TRUE;

        token::Expression::Boolean(token::Boolean {
            token: parser.current_token.clone(),
            value: boolean_value,
        })
    }

    fn parse_prefix_expression(parser: &mut Parser) -> token::Expression {
        let mut lambda_parsers = LambdaParsers {
            prefix_parse_fns: HashMap::new(),
            infix_parse_fns: HashMap::new(),
        };

        lambda_parsers.register_parsers();

        // We have to extract current token and operator
        // Because we'll move to next_token now.
        // To call parse_expression and get `right` expression.
        let token = parser.current_token.clone();
        let operator = parser.current_token.literal.clone();

        parser.next_token();

        // If we enter `parse_expression()` here without `next_token()`
        // we enter the endless loop, followed by stack overflow.
        // parse_expression() -> parse_prefix_expression() -> parse_expression()
        let expression = match parser.parse_expression(&lambda_parsers, token::PREFIX) {
            Some(result) => result,
            None => panic!(
                "Can't parse parser.current_token = {}",
                parser.current_token.literal
            ),
        };

        token::Expression::PrefixExpression(Box::new(token::PrefixExpression {
            token,
            operator,
            right: expression,
        }))
    }

    fn parse_infix_expression(parser: &mut Parser, left: token::Expression) -> token::Expression {
        // TODO: Reinitialization of parser here and in the `parse_prefix_expression`
        // Should move this initialization somewhere and use link everywhere else.
        let mut lambda_parsers = LambdaParsers {
            prefix_parse_fns: HashMap::new(),
            infix_parse_fns: HashMap::new(),
        };

        lambda_parsers.register_parsers();

        let token = parser.current_token.clone();
        let operator = parser.current_token.literal.clone();

        let precedence = precedence_by_token_type(&token.token_type);

        parser.next_token();

        let right = match parser.parse_expression(&lambda_parsers, precedence) {
            Some(parsed_expression) => parsed_expression,
            None => panic!("Cannot find infix parser for {:?}", token),
        };

        // TODO: improve syntax with box-patterns?
        token::Expression::InfixExpression(Box::new(token::InfixExpression {
            token,
            left,
            operator,
            right,
        }))
    }

    #[trace]
    fn parse_grouped_expressions(parser: &mut Parser) -> token::Expression {
        // TODO: Reinitialization of parser here and in the `parse_prefix_expression`
        // Should move this initialization somewhere and use link everywhere else.
        let mut lambda_parsers = LambdaParsers {
            prefix_parse_fns: HashMap::new(),
            infix_parse_fns: HashMap::new(),
        };
        lambda_parsers.register_parsers();

        // If we see `(` we enter here and move cursor to the next token.
        parser.next_token();

        let expression = match parser.parse_expression(&lambda_parsers, token::LOWEST) {
            Some(expression) => expression,
            None => panic!("Cannot find parser for {:?}", parser.current_token),
        };

        if parser.peek_token.token_type != token::RPAREN {
            // TODO: Rework function to properly handle this case.
            // Transofrm this to parser error.
            panic!("I've expected `)`, but got {}", parser.peek_token.token_type);
        } else {
            // it's `)` token, skip it, we already parced expression in `(...)`
            parser.next_token();
        }

        expression
    }
}

#[derive(Debug)]
pub struct Parser {
    lexer: lexer::Lexer,
    current_token: token::Token,
    peek_token: token::Token,
    pub errors: Vec<String>,
}

impl Parser {
    fn new(mut lexer: lexer::Lexer) -> Self {
        let current_token = lexer.next_token();
        let peek_token = lexer.next_token();
        let errors = Vec::new();

        Self {
            lexer,
            current_token,
            peek_token,
            errors,
        }
    }

    fn next_token(&mut self) {
        self.current_token = self.peek_token.clone();
        self.peek_token = self.lexer.next_token();
    }

    // TODO: Current attempt. Move link to LambdaParsers to every function.
    // To avoid double borrowing of self in case of mutual recursive
    // calls.
    pub fn parse_program(&mut self, lambda_parsers: &LambdaParsers) -> Option<ast::Program> {
        let mut program = ast::Program {
            statements: Vec::new(),
        };
        while self.current_token.token_type != token::EOF {
            let statement = self.parse_statement(lambda_parsers);
            match statement {
                Some(stmt) => program.statements.push(stmt),
                _ => (), // Just ignore that case
            };
            self.next_token();
        }
        Some(program)
    }

    fn parse_statement(&mut self, lambda_parsers: &LambdaParsers) -> Option<token::Statements> {
        match self.current_token.token_type.as_ref() {
            token::LET => match self.parse_let_statement() {
                Some(stmt) => Some(token::Statements::LetStatement(stmt)),
                _ => None,
            },
            token::RETURN => match self.parse_return_statement() {
                Some(stmt) => Some(token::Statements::ReturnStatement(stmt)),
                _ => None,
            },
            // If we did not encounter any `let` or `return` it might've happened that
            // we've encountered another type of statement.
            // The last one in our language - expresion statement.
            _ => match self.parse_expression_statement(lambda_parsers) {
                Some(stmt) => Some(token::Statements::ExpressionStatement(stmt)),
                _ => None,
            },
        }
    }

    fn parse_let_statement(&mut self) -> Option<token::LetStatement> {
        let token = self.current_token.clone();

        if self.peek_token.token_type == token::IDENT {
            self.next_token();
        } else {
            self.peek_error(token::IDENT.to_string());
            return None;
        }

        let name = token::Identifier {
            token: self.current_token.clone(),
            value: self.current_token.literal.clone(),
        };

        if self.peek_token.token_type == token::ASSIGN {
            self.next_token();
        } else {
            self.peek_error(token::ASSIGN.to_string());
            return None;
        }

        // TODO: It's a fragile design for now, this code might hang if we don't
        // have a terminating semicolon and next token is token::EOF
        // in this case we'll enter an infinite loop.
        // Doesn't next_token() protect us from this? Apparently - not.
        while !(self.current_token.token_type == token::SEMICOLON) {
            self.next_token(); // skip to next statement in our program
        }

        // TODO: Same happens in return parser. I'm skipping
        // semicolon, so in `parse_statement` function I can
        // just start to parse next value.
        // Weird, it does not work here that way.
        // self.next_token();

        Some(token::LetStatement {
            token,
            name,
            value: "dumb".to_string(),
        })
    }

    fn parse_return_statement(&mut self) -> Option<token::ReturnStatement> {
        let statement = token::ReturnStatement {
            token: self.current_token.clone(),
            return_value: "dumb".to_string(), // How to better describe expression?
        };

        self.next_token();

        while !(self.peek_token.token_type == token::SEMICOLON) {
            self.next_token(); // skip everything till `;` for now
        }

        // TODO: Should I skip semicolon here?
        self.next_token();

        Some(statement)
    }

    fn parse_expression_statement(
        &mut self,
        lambda_parsers: &LambdaParsers,
    ) -> Option<token::ExpressionStatement> {
        let statement = token::ExpressionStatement {
            token: self.current_token.clone(),
            expression: match self.parse_expression(lambda_parsers, token::LOWEST) {
                Some(expression) => expression,
                None => panic!("I don't know how to parse `{}`", self.current_token.literal),
            },
        };

        if self.peek_token.token_type == token::SEMICOLON {
            self.next_token();
        }

        Some(statement)
    }

    #[trace]
    fn parse_expression(
        &mut self,
        lambda_parsers: &LambdaParsers,
        precedence: u8,
    ) -> Option<token::Expression> {
        let prefix_function = lambda_parsers
            .prefix_parse_fns
            .get(&self.current_token.token_type.clone());

        let mut left = match prefix_function {
            Some(PrefixParseFn(prefix_parse_fn)) => prefix_parse_fn(self),
            None => {
                // this step might be redundant, because we check the error above
                self.register_no_prefix_parser_found(self.current_token.token_type.clone());
                return None;
            }
        };

        while !(self.peek_token.token_type == token::SEMICOLON)
            && (precedence < precedence_by_token_type(&self.peek_token.token_type))
        {
            let infix_function = lambda_parsers
                .infix_parse_fns
                .get(&self.peek_token.token_type.clone());

            // change cursor position before calling infix_parse_fn
            // call it with new position
            self.next_token();

            // update left
            left = match infix_function {
                Some(InfixParseFn(infix_parse_fn)) => infix_parse_fn(self, left.clone()),
                None => panic!("Cannot find infix function for {:?}", self.peek_token),
            };
        }

        Some(left)
    }

    fn register_no_prefix_parser_found(&mut self, token_type: token::TokenType) {
        let message = format!("no prefix parser found for {} token", token_type);
        self.errors.push(message);
    }

    fn peek_error(&mut self, token: token::TokenType) {
        let message = format!(
            "expected next token to be {expected}, got {got} instead",
            expected = token,
            got = self.peek_token.token_type,
        );

        self.errors.push(message);
    }
}

#[cfg(test)]
mod tests {
    use crate::ast::Node;
    use crate::lexer;
    use crate::parser::LambdaParsers;
    use crate::parser::Parser;
    use crate::token::Expression;
    use crate::token::Statements;
    use std::collections::HashMap;

    #[test]
    fn test_let_statements() {
        let input = r###"
          let x = 5;
          let y = 10;
          let bebe = 101010;
        "###
        .to_string();

        let lexer = lexer::Lexer::new(input);
        let mut parser = Parser::new(lexer);

        let mut lambda_parsers = LambdaParsers {
            prefix_parse_fns: HashMap::new(),
            infix_parse_fns: HashMap::new(),
        };

        lambda_parsers.register_parsers();

        let program = match parser.parse_program(&lambda_parsers) {
            Some(program) => program,
            None => panic!("Could not parse program"),
        };

        // We would like to accumulate every error in program
        // and later render them to user.
        if !parser.errors.is_empty() {
            println!("Parser encountered {} errors", parser.errors.len());
            for error in parser.errors {
                println!("parser error: {}", error);
            }
            panic!("A few parsing error encountered, see them above.");
        }

        assert_eq!(program.statements.len(), 3);

        let expected = vec!["x".to_string(), "y".to_string(), "bebe".to_string()];

        program
            .statements
            .into_iter()
            .zip(expected.into_iter())
            .for_each(|(statement, expected_identifier)| {
                assert_eq!(statement.token_literal(), "let");

                let let_statement = match statement {
                    Statements::LetStatement(statement) => statement,
                    _ => panic!("I didn't expected anything besides `let` statement"),
                };

                assert_eq!(let_statement.name.value, expected_identifier);

                assert_eq!(let_statement.name.token_literal(), expected_identifier);
            });
    }

    #[test]
    fn test_broken_let_statements_and_check_if_errors_appear() {
        let input = r###"
          let x = 5;
          let y = 10;
          let bebe 101010;
        "###
        .to_string();

        let lexer = lexer::Lexer::new(input);
        let mut parser = Parser::new(lexer);

        let mut lambda_parsers = LambdaParsers {
            prefix_parse_fns: HashMap::new(),
            infix_parse_fns: HashMap::new(),
        };

        lambda_parsers.register_parsers();

        match parser.parse_program(&lambda_parsers) {
            Some(program) => program,
            None => panic!("Could not parse program"),
        };

        // We would like to accumulate every error in program
        // and later render them to user.
        if !parser.errors.is_empty() {
            for error in parser.errors {
                assert_eq!(
                    "parser error: expected next token to be =, got INT instead",
                    format!("parser error: {}", error)
                );
            }
        }
    }

    #[test]
    fn test_return_statement() {
        let input = r###"
          return 1;
          return 111;
        "###
        .to_string();

        let lexer = lexer::Lexer::new(input);
        let mut parser = Parser::new(lexer);

        let mut lambda_parsers = LambdaParsers {
            prefix_parse_fns: HashMap::new(),
            infix_parse_fns: HashMap::new(),
        };

        lambda_parsers.register_parsers();

        let program = match parser.parse_program(&lambda_parsers) {
            Some(program) => program,
            None => panic!("Could not parse program"),
        };

        // We would like to accumulate every error in program
        // and later render them to user.
        if !parser.errors.is_empty() {
            println!("Parser encountered {} errors", parser.errors.len());
            for error in parser.errors {
                println!("parser error: {}", error);
            }
            panic!("A few parsing error encountered, see them above.");
        }

        assert_eq!(program.statements.len(), 2);

        let expected = vec!["1".to_string(), "111".to_string()];

        program
            .statements
            .into_iter()
            .zip(expected.into_iter())
            .for_each(|(statement, expected_identifier)| {
                assert_eq!(statement.token_literal(), "return");

                let return_statement = match statement {
                    Statements::ReturnStatement(statement) => statement,
                    _ => panic!("I didn't expected anything besides `return` statement"),
                };
            });
    }

    #[test]
    fn test_identifier_expression() {
        let input = r###"
          bebe;
        "###
        .to_string();

        let lexer = lexer::Lexer::new(input);
        let mut parser = Parser::new(lexer);

        let mut lambda_parsers = LambdaParsers {
            prefix_parse_fns: HashMap::new(),
            infix_parse_fns: HashMap::new(),
        };

        lambda_parsers.register_parsers();

        let program = match parser.parse_program(&lambda_parsers) {
            Some(program) => program,
            None => panic!("Could not parse program"),
        };

        // We would like to accumulate every error in program
        // and later render them to user.
        if !parser.errors.is_empty() {
            println!("Parser encountered {} errors", parser.errors.len());
            for error in parser.errors {
                println!("parser error: {}", error);
            }
            panic!("A few parsing error encountered, see them above.");
        }

        assert_eq!(program.statements.len(), 1);

        program.statements.into_iter().for_each(|statement| {
            let expression_statement = match statement {
                Statements::ExpressionStatement(statement) => statement,
                _ => panic!("I didn't expect something besides expression statement"),
            };

            let identifier = match expression_statement.expression {
                Expression::Identifier(i) => i,
                _ => panic!("expected to find identifier, but found smth else"),
            };
            // TODO: Maybe pattern matching on concrete branch is better here
            // then some general value() method.
            // Anyway in control code I will use it the other way.
            assert_eq!(identifier.value, "bebe");
            assert_eq!(identifier.token_literal(), "bebe");
        })
    }

    #[test]
    fn test_int_literal_expression() {
        let input = r###"
          42;
        "###
        .to_string();

        let lexer = lexer::Lexer::new(input);
        let mut parser = Parser::new(lexer);

        let mut lambda_parsers = LambdaParsers {
            prefix_parse_fns: HashMap::new(),
            infix_parse_fns: HashMap::new(),
        };

        lambda_parsers.register_parsers();

        let program = match parser.parse_program(&lambda_parsers) {
            Some(program) => program,
            None => panic!("Could not parse program"),
        };

        // We would like to accumulate every error in program
        // and later render them to user.
        if !parser.errors.is_empty() {
            println!("Parser encountered {} errors", parser.errors.len());
            for error in parser.errors {
                println!("parser error: {}", error);
            }
            panic!("A few parsing error encountered, see them above.");
        }

        assert_eq!(program.statements.len(), 1);

        program.statements.into_iter().for_each(|statement| {
            let expression_statement = match statement {
                Statements::ExpressionStatement(statement) => statement,
                _ => panic!("I didn't expect something besides expression statement"),
            };

            let integer_literal = match expression_statement.expression {
                Expression::IntegerLiteral(il) => il,
                _ => panic!("expected to find an integer_literal, but found smth else"),
            };
            // TODO: Maybe pattern matching on concrete branch is better here
            // then some general value() method.
            // Anyway in control code I will use it the other way.
            assert_eq!(integer_literal.value, 42);
            assert_eq!(integer_literal.token_literal(), "42");
        })
    }

    #[test]
    fn test_prefix_expressions() {
        let inputs = ["!10;".to_string(), "-101010;".to_string()];

        let expected = vec![("!".to_string(), 10), ("-".to_string(), 101010)];

        // Iterate over every prefix expression and test it individualy
        inputs
            .into_iter()
            .zip(expected.into_iter())
            .for_each(|(input, token_pair)| {
                let lexer = lexer::Lexer::new(input.to_string());
                let mut parser = Parser::new(lexer);

                let mut lambda_parsers = LambdaParsers {
                    prefix_parse_fns: HashMap::new(),
                    infix_parse_fns: HashMap::new(),
                };

                lambda_parsers.register_parsers();

                let program = match parser.parse_program(&lambda_parsers) {
                    Some(program) => program,
                    None => panic!("Could not parse program"),
                };

                // We would like to accumulate every error in program
                // and later render them to user.
                if !parser.errors.is_empty() {
                    println!("Parser encountered {} errors", parser.errors.len());
                    for error in parser.errors {
                        println!("parser error: {}", error);
                    }
                    panic!("A few parsing error encountered, see them above.");
                }

                assert_eq!(program.statements.len(), 1);

                let expression_statement = match &program.statements[0] {
                    Statements::ExpressionStatement(statement) => statement,
                    _ => panic!("I didn't expected anything besides `expression` statement"),
                };

                let prefix_expression = match &expression_statement.expression {
                    Expression::PrefixExpression(pe) => pe,
                    _ => panic!("I've expected prefix expression here - sorry"),
                };

                let operator = token_pair.0;
                let integer = token_pair.1;

                assert_eq!(prefix_expression.operator, operator);
                assert_integer_literal(&prefix_expression.right, integer);
            });
    }

    #[test]
    fn test_infix_expressions() {
        let inputs = [
            "1 + 1;".to_string(),
            "1 - 1;".to_string(),
            "1 * 1;".to_string(),
            "1 / 1;".to_string(),
            "1 > 1;".to_string(),
            "1 < 1;".to_string(),
            "1 == 1;".to_string(),
            "1 != 1;".to_string(),
        ];

        let expected = vec![
            (1, "+".to_string(), 1),
            (1, "-".to_string(), 1),
            (1, "*".to_string(), 1),
            (1, "/".to_string(), 1),
            (1, ">".to_string(), 1),
            (1, "<".to_string(), 1),
            (1, "==".to_string(), 1),
            (1, "!=".to_string(), 1),
        ];

        // Iterate over every prefix expression and test it individualy
        inputs.into_iter().zip(expected.into_iter()).for_each(
            |(input, (left_integer, operator, right_integer))| {
                let lexer = lexer::Lexer::new(input.to_string());
                let mut parser = Parser::new(lexer);

                let mut lambda_parsers = LambdaParsers {
                    prefix_parse_fns: HashMap::new(),
                    infix_parse_fns: HashMap::new(),
                };

                lambda_parsers.register_parsers();

                let program = match parser.parse_program(&lambda_parsers) {
                    Some(program) => program,
                    None => panic!("Could not parse program"),
                };

                // We would like to accumulate every error in program
                // and later render them to user.
                if !parser.errors.is_empty() {
                    println!("Parser encountered {} errors", parser.errors.len());
                    for error in parser.errors {
                        println!("parser error: {}", error);
                    }
                    panic!("A few parsing error encountered, see them above.");
                }

                assert_eq!(program.statements.len(), 1);

                let expression_statement = match &program.statements[0] {
                    Statements::ExpressionStatement(statement) => statement,
                    _ => panic!("I didn't expected anything besides `expression` statement"),
                };

                let infix_expression = match &expression_statement.expression {
                    Expression::InfixExpression(ie) => ie,
                    _ => panic!("I've expected infix expression here - sorry"),
                };

                assert_integer_literal(&infix_expression.left, left_integer);
                assert_eq!(infix_expression.operator, operator);
                assert_integer_literal(&infix_expression.right, right_integer);
            },
        );
    }

    #[test]
    fn test_boolean_expressions() {
        let inputs = ["true;".to_string(), "false;".to_string()];

        let expected = vec![true, false];

        // Iterate over every prefix expression and test it individualy
        inputs
            .into_iter()
            .zip(expected.into_iter())
            .for_each(|(input, boolean_value)| {
                let lexer = lexer::Lexer::new(input.to_string());
                let mut parser = Parser::new(lexer);

                let mut lambda_parsers = LambdaParsers {
                    prefix_parse_fns: HashMap::new(),
                    infix_parse_fns: HashMap::new(),
                };

                lambda_parsers.register_parsers();

                let program = match parser.parse_program(&lambda_parsers) {
                    Some(program) => program,
                    None => panic!("Could not parse program"),
                };

                // We would like to accumulate every error in program
                // and later render them to user.
                if !parser.errors.is_empty() {
                    println!("Parser encountered {} errors", parser.errors.len());
                    for error in parser.errors {
                        println!("parser error: {}", error);
                    }
                    panic!("A few parsing error encountered, see them above.");
                }

                assert_eq!(program.statements.len(), 1);

                let expression_statement = match &program.statements[0] {
                    Statements::ExpressionStatement(statement) => statement,
                    _ => panic!("I didn't expected anything besides `expression` statement"),
                };

                let boolean_expression = match &expression_statement.expression {
                    Expression::Boolean(b) => b,
                    _ => panic!("I've expected boolean expression here - sorry"),
                };

                assert_eq!(boolean_expression.value, boolean_value);
            });
    }

    #[test]
    fn test_operator_precedence() {
        let inputs = [
            "(1 + 2) * 3 + 4;".to_string(),
            "!true == false;".to_string(),
        ];

        let expected = [
            "(((1 + 2) * 3) + 4)\n".to_string(),
            "((! true) == false)\n".to_string(),
        ];

        // Iterate over every prefix expression and test it individualy
        inputs.into_iter().zip(expected.into_iter()).for_each(|(input, expected)| {
            let lexer = lexer::Lexer::new(input.to_string());
            let mut parser = Parser::new(lexer);

            let mut lambda_parsers = LambdaParsers {
                prefix_parse_fns: HashMap::new(),
                infix_parse_fns: HashMap::new(),
            };

            lambda_parsers.register_parsers();

            let program = match parser.parse_program(&lambda_parsers) {
                Some(program) => program,
                None => panic!("Could not parse program"),
            };

            // We would like to accumulate every error in program
            // and later render them to user.
            if !parser.errors.is_empty() {
                println!("Parser encountered {} errors", parser.errors.len());
                for error in parser.errors {
                    println!("parser error: {}", error);
                }
                panic!("A few parsing error encountered, see them above.");
            }

            // This and a couple of next tests will be run with
            // stringification in mind. Like this assertion:
            assert_eq!(program.to_string(), *expected);
        });
    }

    // <<-- HELPER ASSERTIONS -->>
    fn assert_integer_literal(expression: &Expression, expected: i32) {
        let integer = match expression {
            Expression::IntegerLiteral(il) => il,
            _ => panic!("fail in assert_integer_literal"),
        };

        assert_eq!(integer.value, expected);
    }
}
