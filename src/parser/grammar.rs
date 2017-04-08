//! Grammar AST parser.
//!
//! Many methods in this module return `IResult<&str, O>` as an argument. It is
//! actually an enum defined by `nom` which represents parsing result.
//! `IResult::Done` is a tuple enum value where the first argument is
//! text left to parse, and second is retrieved AST value.
//! `IResult::Error` means that parse did fail.

use parser::ast::{BinaryOperator, Expression, Label, Number, NumberWidth, Opcode, OpcodeMode,
                  Statement, VariableName};

use std::str::{self, FromStr};

use nom;
pub use nom::IResult;
use unicode_xid::UnicodeXID;

fn valid_identifier_first_character(result: &str) -> bool {
    let message = "take_s did not take one parameter";
    let result = result.chars().next().expect(message);
    result == '!' || result == '_' || UnicodeXID::is_xid_start(result)
}

fn valid_later_character(c: char) -> bool {
    UnicodeXID::is_xid_continue(c) || c == '_'
}

const OPERATORS: &'static str = "+-*/";

named!(
/// An identifier parser.
///
/// It allows any Unicode identifier as specified by [Unicode Standard Annex #31:
/// Unicode Identifier and Pattern Syntax](http://www.unicode.org/reports/tr31/tr31-9.html).
/// In addition, it allows use of underscores in identifiers, as it is tradition
/// in C like programming languages, and ! at beginning (like xkas and Asar require,
/// but not MVP).
///
/// # Examples
///
/// Parsing an Unicode identifier.
///
///     use mvp::parser::grammar::{self, IResult};
///
///     let parsed = grammar::identifier("世界");
///     assert_eq!(parsed, IResult::Done("", String::from("世界").into_boxed_str()));
,
pub identifier<&str, Box<str>>, do_parse!(
    first: verify!(take_s!(1), valid_identifier_first_character) >>
    res: take_while_s!(valid_later_character) >>
    (format!("{}{}", first, res).into_boxed_str())
));

named!(pub statement<&str, Statement>, ws!(alt!(
    opcode => { |opcode| Statement::Opcode(opcode) }
)));

named!(immediate<&str, (Expression, OpcodeMode)>, ws!(do_parse!(
    tag!("#") >>
    expression: expression >>
    (expression, OpcodeMode::Immediate)
)));

named!(indirect<&str, (Expression, OpcodeMode)>, ws!(do_parse!(
    tag!("(") >>
    expression: expression >>
    tag!(")") >>
    not!(one_of!(OPERATORS)) >>
    (expression, OpcodeMode::Indirect)
)));

named!(x_indirect<&str, (Expression, OpcodeMode)>, ws!(do_parse!(
    tag!("(") >>
    expression: expression >>
    tag!(",") >>
    one_of!("xX") >>
    tag!(")") >>
    (expression, OpcodeMode::XIndirect)
)));

named!(indirect_y<&str, (Expression, OpcodeMode)>, ws!(do_parse!(
    tag!("(") >>
    expression: expression >>
    tag!(")") >>
    tag!(",") >>
    one_of!("yY") >>
    (expression, OpcodeMode::IndirectY)
)));

named!(stack_indirect_y<&str, (Expression, OpcodeMode)>, ws!(do_parse!(
    tag!("(") >>
    expression: expression >>
    tag!(",") >>
    one_of!("sS") >>
    tag!(")") >>
    tag!(",") >>
    one_of!("yY") >>
    (expression, OpcodeMode::StackIndirectY)
)));

named!(long_indirect<&str, (Expression, OpcodeMode)>, ws!(do_parse!(
    tag!("[") >>
    expression: expression >>
    tag!("]") >>
    (expression, OpcodeMode::LongIndirect)
)));

named!(long_indirect_y<&str, (Expression, OpcodeMode)>, ws!(do_parse!(
    res: long_indirect >>
    tag!(",") >>
    one_of!("yY") >>
    (res.0, OpcodeMode::LongIndirectY)
)));

named!(address_pair<&str, (Expression, OpcodeMode)>, do_parse!(
    first: expression >>
    tag!(",") >>
    second: expression >>
    (first, OpcodeMode::Move { second: second })
));

named!(address<&str, (Expression, OpcodeMode)>, do_parse!(
    expression: expression >>
    (expression, OpcodeMode::Address)
));

named!(opcode<&str, Opcode>, do_parse!(
    opcode: identifier >>
    mode: opt!(ws!(pair!(tag!("."), one_of!("bBwWlL")))) >>
    result: alt!(
        indirect_y |
        indirect |
        x_indirect |
        address_pair |
        address |
        immediate |
        long_indirect_y |
        long_indirect |
        stack_indirect_y
    ) >>
    (Opcode {
        name: opcode,
        width: mode.map(|(_, letter)| match letter {
            'b'|'B' => 1,
            'w'|'W' => 2,
            'l'|'L' => 3,
            _ => unreachable!(),
        }),
        value: result.0,
        mode: result.1,
    })
));

named!(
/// Assignment statement parser.
///
/// It expects variable name, followed by `=` character, and an expression
/// which marks expression to be stored as value.
///
/// # Examples
///
///     use mvp::parser::grammar::{self, IResult};
///     use mvp::parser::ast::{Expression, Number, NumberWidth, Statement, VariableName};
///
///     let parsed = grammar::assignment("hello = 44");
///     let expected = Statement::Assignment(
///         VariableName(String::from("hello").into_boxed_str()),
///         Expression::Number(Number { value: 44, width: NumberWidth::None }),
///     );
///     assert_eq!(parsed, IResult::Done("", expected));
,
pub assignment<&str, Statement>, ws!(do_parse!(
    name: identifier >>
    tag!("=") >>
    value: expression >>
    (Statement::Assignment(VariableName(name), value))
)));

named!(label<&str, Label>, map!(identifier, |name| Label::Named(VariableName(name))));

named!(
/// An expression parser.
///
/// This can be used as math expression parser, however due to language
/// limitations, it doesn't support types like decimal numbers.
/// However, it does support mathematical operators like addition,
/// subtraction, multiplication and division, as well as parenthesis.
///
/// # Example
///
/// Parsing a mathematical expression:
///
///     use mvp::parser::grammar::{self, IResult};
///     use mvp::parser::ast::{BinaryOperator, Expression, Number, NumberWidth};
///
///     let parsed = grammar::expression("2 + 3");
///     let expected = IResult::Done("", Expression::Binary(
///         BinaryOperator::Add,
///         Box::new([
///             Expression::Number(Number { value: 2, width: NumberWidth::None }),
///             Expression::Number(Number { value: 3, width: NumberWidth::None }),
///         ]),
///     ));
///     assert_eq!(parsed, expected);
,
pub expression<&str, Expression>, ws!(do_parse!(
    init: term >>
    res: fold_many0!(
        pair!(alt!(
            tag!("+") => {|_| BinaryOperator::Add}
            | tag!("-") => {|_| BinaryOperator::Sub}
        ), term),
        init,
        |first, (operator, another)| {
            Expression::Binary(operator, Box::new([first, another]))
        }
    ) >>
    (res)
)));

named!(term<&str, Expression>, do_parse!(
    init: top_expression >>
    res: fold_many0!(
        pair!(alt!(
            tag!("*") => {|_| BinaryOperator::Mul}
            | tag!("/") => {|_| BinaryOperator::Div}
        ), top_expression),
        init,
        |first, (operator, another)| {
            Expression::Binary(operator, Box::new([first, another]))
        }
    ) >>
    (res)
));

named!(top_expression<&str, Expression>, alt!(
    paren_expression |
    number |
    hex_number |
    call |
    variable
));

named!(paren_expression<&str, Expression>, ws!(delimited!(tag!("("), expression, tag!(")"))));

named!(number<&str, Expression>, map!(
    map_res!(
        ws!(nom::digit),
        u32::from_str
    ),
    |value| Expression::Number(Number { value: value, width: NumberWidth::None })
));

fn hex_width_for_length(length: usize) -> NumberWidth {
    match length {
        2 => NumberWidth::OneByte,
        4 => NumberWidth::TwoBytes,
        _ => NumberWidth::None,
    }
}

named!(hex_number<&str, Expression>, ws!(do_parse!(
    tag!("$") >>
    number: map!(
        map_res!(nom::hex_digit, |s| u32::from_str_radix(s, 16).map(|value| (s.len(), value))),
        |(length, value)| Expression::Number(Number {
            value: value,
            width: hex_width_for_length(length),
        })
    ) >>
    (number)
)));

named!(call<&str, Expression>, ws!(do_parse!(
    identifier: identifier >>
    parts: delimited!(
        tag!("("),
        separated_list!(tag!(","), expression),
        tag!(")")
    ) >>
    (Expression::Call(VariableName(identifier), parts))
)));

named!(variable<&str, Expression>, map!(label, Expression::Variable));
