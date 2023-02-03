use crate::parser::Rule;
use colored::{Color, Colorize};
use pest::{iterators::Pair, RuleType};
use std::fmt::{Display, Result as FmtResult, Write};

#[derive(Debug)]
pub struct ModelError(String);

impl ModelError {
    fn from_str(msg: &str) -> Self {
        Self(msg.to_owned())
    }
}

fn assert_rule<'a>(pair: Pair<'a, Rule>, rule: Rule) -> Result<Pair<'a, Rule>, ModelError> {
    if pair.as_rule() == rule {
        Ok(pair)
    } else {
        Err(ModelError(format!(
            "Expected pair {:?} rule to be {:?}",
            pair, rule
        )))
    }
}

#[derive(Clone, Debug)]
pub enum Expression {
    Terminal {
        name: String,
        value: Option<String>,
    },
    NonTerminal {
        name: String,
        children: Vec<Expression>,
    },
}

impl Expression {
    pub fn name(&self) -> &String {
        match self {
            Self::Terminal { name, value: _ } => name,
            Self::NonTerminal { name, children: _ } => name,
        }
    }
}

impl Expression {
    pub fn try_from_sexpr<'a>(pair: Pair<'a, Rule>) -> Result<Self, ModelError> {
        let mut inner = pair.into_inner();
        let name = inner
            .next()
            .ok_or_else(|| ModelError::from_str("Missing rule name"))
            .and_then(|pair| assert_rule(pair, Rule::rule_name))
            .map(|pair| pair.as_str().to_owned())?;
        match inner.next() {
            None => Ok(Self::Terminal { name, value: None }),
            Some(pair) => match pair.as_rule() {
                Rule::sub_expressions => {
                    let children: Result<Vec<Expression>, ModelError> = pair
                        .into_inner()
                        .map(|pair| Self::try_from_sexpr(pair))
                        .collect();
                    Ok(Self::NonTerminal {
                        name,
                        children: children?,
                    })
                }
                Rule::rule_value_str => {
                    let value = pair
                        .into_inner()
                        .next()
                        .map(|pair| assert_rule(pair, Rule::rule_value))
                        .transpose()
                        .map(|opt| {
                            opt.map(|pair| pair.as_str().to_owned())
                                .or_else(|| Some(String::new()))
                        })?;
                    Ok(Self::Terminal { name, value })
                }
                other => Err(ModelError(format!("Unexpected rule {:?}", other))),
            },
        }
    }

    pub fn try_from_code<'a, R: RuleType>(pair: Pair<'a, R>) -> Result<Self, ModelError> {
        let name = format!("{:?}", pair.as_rule());
        let value = pair.as_str();
        let children: Result<Vec<Expression>, ModelError> = pair
            .into_inner()
            .map(|pair| Self::try_from_code(pair))
            .collect();
        match children {
            Ok(children) if children.is_empty() => Ok(Self::Terminal {
                name,
                value: Some(value.to_owned()),
            }),
            Ok(children) => Ok(Self::NonTerminal {
                name,
                children: children,
            }),
            Err(e) => Err(e),
        }
    }
}

pub struct ExpressionFormatter<'a> {
    writer: &'a mut dyn Write,
    indent: &'a str,
    pub(crate) level: usize,
    pub(crate) color: Option<Color>,
}

impl<'a> ExpressionFormatter<'a> {
    pub fn from_defaults(writer: &'a mut dyn Write) -> Self {
        Self {
            writer,
            indent: "  ",
            level: 0,
            color: None,
        }
    }

    pub(crate) fn write_indent(&mut self) -> FmtResult {
        for _ in 0..self.level {
            self.writer.write_str(self.indent)?;
        }
        Ok(())
    }

    pub(crate) fn write_char(&mut self, c: char) -> FmtResult {
        match self.color {
            Some(color) => self.writer.write_str(c.to_string().color(color).as_ref()),
            None => self.writer.write_char(c),
        }
    }

    pub(crate) fn write_newline(&mut self) -> FmtResult {
        self.writer.write_char('\n')
    }

    pub(crate) fn write_str(&mut self, s: &str) -> FmtResult {
        match self.color {
            Some(color) => self.writer.write_str(s.color(color).as_ref()),
            None => self.writer.write_str(s),
        }
    }

    pub fn fmt(&mut self, expression: &Expression) -> FmtResult {
        self.write_indent()?;
        self.write_char('(')?;
        match expression {
            Expression::Terminal { name, value } => {
                self.write_str(name)?;
                if let Some(value) = value {
                    self.write_str(": ")?;
                    self.write_str(value)?;
                }
                self.write_char(')')?;
            }
            Expression::NonTerminal { name, children } if children.is_empty() => {
                self.write_str(name)?;
                self.write_char(')')?;
            }
            Expression::NonTerminal { name, children } => {
                self.write_str(name)?;
                self.write_newline()?;
                self.level += 1;
                for child in children {
                    self.fmt(child)?;
                    self.write_newline()?;
                }
                self.level -= 1;
                self.write_indent()?;
                self.write_char(')')?;
            }
        }
        Ok(())
    }
}

impl Display for Expression {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> FmtResult {
        ExpressionFormatter::from_defaults(f).fmt(self)
    }
}

#[derive(Clone, Debug)]
pub struct TestCase {
    pub name: String,
    pub code: String,
    pub expression: Expression,
}

impl TestCase {
    pub fn try_from_pair<'a>(pair: Pair<'a, Rule>) -> Result<Self, ModelError> {
        let mut inner = pair.into_inner();
        let name = inner
            .next()
            .ok_or_else(|| ModelError::from_str("Missing test name"))
            .and_then(|pair| assert_rule(pair, Rule::test_name))
            .map(|pair| pair.as_str().trim().to_owned())?;
        let mut code_block = inner
            .next()
            .ok_or_else(|| ModelError::from_str("Missing code block"))
            .and_then(|pair| assert_rule(pair, Rule::code_block))
            .map(|pair| pair.into_inner())?;
        code_block
            .next()
            .ok_or_else(|| ModelError::from_str("Missing div"))
            .and_then(|pair| assert_rule(pair, Rule::div))?;
        let code = code_block
            .next()
            .ok_or_else(|| ModelError::from_str("Missing code"))
            .and_then(|pair| assert_rule(pair, Rule::code))
            .map(|pair| pair.as_str().trim().to_owned())?;
        let expression = inner
            .next()
            .ok_or_else(|| ModelError::from_str("Missing expression"))
            .and_then(|pair| assert_rule(pair, Rule::expression))?;
        Ok(TestCase {
            name,
            code,
            expression: Expression::try_from_sexpr(expression)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Expression, TestCase};
    use crate::{
        parser::{Rule, TestParser},
        Error,
    };
    use indoc::indoc;

    fn assert_nonterminal<'a>(
        expression: &'a Expression,
        expected_name: &'a str,
    ) -> &'a Vec<Expression> {
        match expression {
            Expression::NonTerminal { name, children } => {
                assert_eq!(name, expected_name);
                children
            }
            _ => panic!("Expected non-terminal expression but found {expression:?}"),
        }
    }

    fn assert_terminal(expression: &Expression, expected_name: &str, expected_value: Option<&str>) {
        match expression {
            Expression::Terminal { name, value } => {
                assert_eq!(name, expected_name);
                match (value, expected_value) {
                    (Some(actual), Some(expected)) => assert_eq!(actual, expected),
                    (Some(actual), None) => {
                        panic!("Terminal node has value {actual} but there is no expected value")
                    }
                    (None, Some(expected)) => {
                        panic!("Terminal node has no value but expected {expected}")
                    }
                    _ => (),
                }
            }
            _ => panic!("Expected non-terminal expression but found {expression:?}"),
        }
    }

    #[test]
    fn test_parse_into() -> Result<(), Error<Rule>> {
        let text = indoc! {r#"
        My Test

        =======
  
        fn x() int {
          return 1;
        }
  
        =======
        
        (source_file
          (function_definition
            (identifier: "x")
            (parameter_list)
            (primitive_type: "int")
            (block
              (return_statement 
                (number: "1")
              )
            )
          )
        )
        "#};
        let test_case: TestCase = TestParser::parse(text)
            .map_err(|source| Error::Parser { source })
            .and_then(|pair| {
                TestCase::try_from_pair(pair).map_err(|source| Error::Model { source })
            })?;
        assert_eq!(test_case.name, "My Test");
        assert_eq!(test_case.code, "fn x() int {\n  return 1;\n}");
        let expression = test_case.expression;
        let children = assert_nonterminal(&expression, "source_file");
        assert_eq!(children.len(), 1);
        let children = assert_nonterminal(&children[0], "function_definition");
        assert_eq!(children.len(), 4);
        assert_terminal(&children[0], "identifier", Some("x"));
        assert_terminal(&children[1], "parameter_list", None);
        assert_terminal(&children[2], "primitive_type", Some("int"));
        let children = assert_nonterminal(&children[3], "block");
        assert_eq!(children.len(), 1);
        let children = assert_nonterminal(&children[0], "return_statement");
        assert_eq!(children.len(), 1);
        assert_terminal(&children[0], "number", Some("1"));
        Ok(())
    }
}