use super::function::Function;
use super::list::parse_list;
use super::parse_string;
use nom::IResult;
use nom_leb128::leb128_usize;

#[derive(Debug)]
pub struct Chunk {
    pub string_table: Vec<String>,
    pub functions: Vec<Function>,
    pub main: usize,
}

impl Chunk {
    pub(crate) fn parse(input: &[u8]) -> IResult<&[u8], Self> {
        let (input, string_table) = parse_list(input, parse_string)?;
        let (input, functions) = parse_list(input, Function::parse)?;
        let (input, main) = leb128_usize(input)?;

        Ok((
            input,
            Self {
                string_table,
                functions,
                main
            },
        ))
    }
}