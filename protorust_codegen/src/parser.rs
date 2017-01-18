use std::io::Result as IoResult;
use std::io::Write;
use nom::IError;

#[derive(Debug)]
enum Frequency {
    Optional,
    Repeated,
    Required,
}

#[derive(Debug)]
struct Field<'a> {
    name: &'a str,
    frequency: Frequency,
    typ: &'a str,
    number: i32,
    default: Option<&'a str>,
}

impl<'a> Field<'a> {

    fn rust_type(&self) -> &str {
        match self.typ {
            "int32" | "sint32" | "sfixed32" => "i32",
            "int64" | "sint64" | "sfixed64" => "i64",
            "uint32" | "fixed32" => "u32",
            "uint64" | "fixed64" => "u64",
            "float" => "f32",
            "double" => "f64",
            "string" => "String",
            "bytes" => "Vec<u8>",
            t => t,
        }
    }

    fn wire_type(&self, enums: &[&str]) -> &str {
        match self.typ {
            "int32" | "sint32" | "int64" | "sint64" | 
                "uint32" | "uint64" | "bool" | "enum" => "WireType::Varint",
            "fixed64" | "sfixed64" | "double" => "WireType::Fixed64",
            "fixed32" | "sfixed32" | "float" => "WireType::Fixed32",
            t if enums.contains(&t) => "WireType::Varint",
            _ => "WireType::LengthDelimited",
        }
    }

    fn read_fn(&self, enums: &[&str]) -> &str {
        match self.typ {
            "int32" | "sint32" | "int64" | "sint64" | 
                "uint32" | "uint64" | "bool" | "fixed64" | 
                "sfixed64" | "double" | "fixed32" | "sfixed32" | 
                "float" | "string" | "bytes" => self.typ,
            t if enums.contains(&t) => "enum",
            _ => "message",
        }
    }

    fn write_definition<W: Write>(&self, w: &mut W) -> IoResult<()> {
        match self.frequency {
            Frequency::Optional => writeln!(w, "    pub {}: Option<{}>,", self.name, self.rust_type()),
            Frequency::Repeated => writeln!(w, "    pub {}: Vec<{}>,", self.name, self.rust_type()),
            Frequency::Required => writeln!(w, "    pub {}: {},", self.name, self.rust_type()),
        }
    }

    fn write_match_tag<W: Write>(&self, w: &mut W, enums: &[&str]) -> IoResult<()> {
        match self.frequency {
            Frequency::Optional => {
                writeln!(w, "({}, {}) => msg.{} = Some(r.read_{}()?),",
                    self.number, self.wire_type(enums), self.name, self.read_fn(enums))
            } 
            Frequency::Repeated => {
                writeln!(w, "({}, {}) => msg.{}.push(r.read_{}()?),",
                    self.number, self.wire_type(enums), self.name, self.read_fn(enums))
            }
            Frequency::Required => {
                writeln!(w, "({}, {}) => msg.{} = r.read_{}()?,",
                    self.number, self.wire_type(enums), self.name, self.read_fn(enums))
            }
        }
    }

    fn write_match_not_tag<W: Write>(&self, w: &mut W, name: &str) -> IoResult<()> {
        writeln!(w, "({}, _) => return Err(ErrorKind::InvalidMessage(tag, \"{} {}\").into()),",
            self.number, name, self.typ)
    }
}

#[derive(Debug)]
pub struct Message<'a> {
    name: &'a str,
    fields: Vec<Field<'a>>,
}

impl<'a> Message<'a> {
    fn write_definition<W: Write>(&self, w: &mut W) -> IoResult<()> {
        writeln!(w, "#[derive(Debug, Default, PartialEq, Clone)]")?;
        writeln!(w, "pub struct {} {{", self.name)?;
        for f in &self.fields {
            f.write_definition(w)?;
        }
        writeln!(w, "}}")
    }

    fn write_impl_message<W: Write>(&self, w: &mut W, enums: &[&str]) -> IoResult<()> {
        writeln!(w, "impl Message for {} {{", self.name)?;
        writeln!(w, "    fn from_reader<R: BufRead>(mut r: &mut Reader<R>) -> Result<Self> {{")?;
        writeln!(w, "        let mut msg = Self::default();")?;
        writeln!(w, "        loop {{")?;
        writeln!(w, "            let tag = match r.next_tag() {{")?;
        writeln!(w, "                None => break,")?;
        writeln!(w, "                Some(Err(e)) => return Err(e),")?;
        writeln!(w, "                Some(Ok(tag)) => tag,")?;
        writeln!(w, "            }};")?;
        writeln!(w, "            match tag.unpack() {{")?;
        for f in &self.fields {
            write!(w, "                 ")?;
            f.write_match_tag(w, enums)?;
        }
        for f in &self.fields {
            write!(w, "                 ")?;
            f.write_match_not_tag(w, self.name)?;
        }
        writeln!(w, "                (_, wire_type) => r.read_unknown(wire_type)?,")?;
        writeln!(w, "            }}")?;
        writeln!(w, "        }}")?;
        writeln!(w, "        Ok(msg)")?;
        writeln!(w, "    }}")?;
        writeln!(w, "}}")
    }
}

#[derive(Debug)]
pub struct Enumerator<'a> {
    name: &'a str,
    fields: Vec<(&'a str, i32)>,
}

impl<'a> Enumerator<'a> {
    fn write_definition<W: Write>(&self, w: &mut W) -> IoResult<()> {
        writeln!(w, "#[derive(Debug, PartialEq, Eq, Clone)]")?;
        writeln!(w, "pub enum {} {{", self.name)?;
        for &(f, _) in &self.fields {
            writeln!(w, "    {},", f)?;
        }
        writeln!(w, "}}")
    }

    fn write_impl_default<W: Write>(&self, w: &mut W) -> IoResult<()> {
        writeln!(w, "impl Default for {} {{", self.name)?;
        writeln!(w, "    fn default() -> Self {{")?;
        // TODO: check with default field and return error if there is no field
        writeln!(w, "        {}::{}", self.name, self.fields[0].0)?;
        writeln!(w, "    }}")?;
        writeln!(w, "}}")
    }

    fn write_from_u64<W: Write>(&self, w: &mut W) -> IoResult<()> {
        writeln!(w, "impl From<u64> for {} {{", self.name)?;
        writeln!(w, "    fn from(i: u64) -> Self {{")?;
        writeln!(w, "        match i {{")?;
        for &(f, number) in &self.fields {
            writeln!(w, "            {} => {}::{},", number, self.name, f)?;
        }
        writeln!(w, "            _ => Self::default(),")?;
        writeln!(w, "        }}")?;
        writeln!(w, "    }}")?;
        writeln!(w, "}}")
    }
}

#[derive(Debug)]
pub enum MessageOrEnum<'a> {
    Msg(Message<'a>),
    Enum(Enumerator<'a>),
    Ignore,
}

#[derive(Debug)]
pub struct FileDescriptor<'a> {
    message_and_enums: Vec<MessageOrEnum<'a>>,
}

impl<'a> FileDescriptor<'a> {
    pub fn from_str(f: &'a str) -> Result<FileDescriptor<'a>, IError<u32>> {
        FileDescriptor::from_bytes(f.as_bytes())

    }
    pub fn from_bytes(b: &'a [u8]) -> Result<FileDescriptor<'a>, IError<u32>> {
        self::parser::file_descriptor(b).to_full_result()
    }

    pub fn write<W: Write>(&self, w: &mut W, filename: &str) -> IoResult<()> {
        
        writeln!(w, "//! Automatically generated rust module for '{}' file", filename)?;
        writeln!(w, "")?;
        writeln!(w, "#![allow(non_snake_case)]")?;
        writeln!(w, "#![allow(non_upper_case_globals)]")?;
        writeln!(w, "")?;
        writeln!(w, "use std::io::BufRead;")?;
        writeln!(w, "use quick_protobuf::errors::{{Result, ErrorKind}};")?;
        writeln!(w, "use quick_protobuf::message::Message;")?;
        writeln!(w, "use quick_protobuf::reader::Reader;")?;
        writeln!(w, "use quick_protobuf::types::WireType;")?;

        let enums = self.message_and_enums.iter().filter_map(|m| {
            if let &MessageOrEnum::Enum(ref e) = m {
                Some(e.name)
            } else {
                None
            }
        }).collect::<Vec<_>>();
        for m in &self.message_and_enums {
            match m {
                &MessageOrEnum::Msg(ref m) => {
                    writeln!(w, "")?;
                    m.write_definition(w)?;
                    writeln!(w, "")?;
                    m.write_impl_message(w, &enums)?;
                },
                &MessageOrEnum::Enum(ref m) => {
                    writeln!(w, "")?;
                    m.write_definition(w)?;
                    writeln!(w, "")?;
                    m.write_impl_default(w)?;
                    writeln!(w, "")?;
                    m.write_from_u64(w)?;
                },
                &MessageOrEnum::Ignore => continue,
            }
        }
        Ok(())
    }
}

mod parser {

    use super::*;
    use std::str;
    use nom::{alphanumeric, multispace, digit};

    named!(comment<()>, do_parse!(tag!("//") >> take_until_and_consume!("\n") >> ()));

    /// word break: multispace or comment
    named!(br<()>, alt!(map!(multispace, |_| ()) | comment));

    named!(default_value<&str>, do_parse!(
        tag!("[") >> many0!(br) >> tag!("default") >> many0!(br) >> tag!("=") >> many0!(br) >> 
        default: map_res!(alphanumeric, str::from_utf8) >> many0!(br) >> tag!("]") >>
        (default)));

    named!(message_field<Field>, do_parse!(
        frequency: alt!(map!(tag!("optional"), |_| Frequency::Optional) |
                        map!(tag!("repeated"), |_| Frequency::Repeated) |
                        map!(tag!("required"), |_| Frequency::Required)) >> many1!(br) >>
        typ: map_res!(alphanumeric, str::from_utf8) >> many1!(br) >>
        name: map_res!(alphanumeric, str::from_utf8) >> many0!(br) >>
        tag!("=") >> many0!(br) >>
        number: map_res!(map_res!(digit, str::from_utf8), str::FromStr::from_str) >> many0!(br) >> 
        default: opt!(default_value) >> many0!(br) >> tag!(";") >> many0!(br) >>
        (Field {
           name: name,
           frequency: frequency,
           typ: typ,
           number: number,
           default: default,
        })));

    named!(message<Message>, do_parse!(
        tag!("message") >> many0!(br) >> 
        name: map_res!(alphanumeric, str::from_utf8) >> many0!(br) >> 
        tag!("{") >> many0!(br) >>
        fields: many0!(message_field) >> 
        tag!("}") >> many0!(br) >>
        (Message { name: name, fields: fields })));

    named!(enum_field<(&str, i32)>, do_parse!(
        name: map_res!(alphanumeric, str::from_utf8) >> many0!(br) >>
        tag!("=") >> many0!(br) >>
        number: map_res!(map_res!(digit, str::from_utf8), str::FromStr::from_str) >> many0!(br) >>
        tag!(";") >> many0!(br) >>
        ((name, number))));
        
    named!(enumerator<Enumerator>, do_parse!(
        tag!("enum") >> many1!(br) >>
        name: map_res!(alphanumeric, str::from_utf8) >> many0!(br) >>
        tag!("{") >> many0!(br) >>
        fields: many0!(enum_field) >> 
        tag!("}") >> many0!(br) >>
        (Enumerator { name: name, fields: fields })));

    named!(ignore<()>, do_parse!(
        alt!(tag!("package") | tag!("option")) >> many1!(br) >> 
        take_until_and_consume!(";") >> many0!(br) >> ()));

    named!(message_or_enum<MessageOrEnum>, alt!(
             message => { |m| MessageOrEnum::Msg(m) } | 
             enumerator => { |e| MessageOrEnum::Enum(e) } |
             ignore => { |_| MessageOrEnum::Ignore } ));

    named!(pub file_descriptor<FileDescriptor>, do_parse!(
        many0!(br) >>
        message_and_enums: many0!(message_or_enum) >>
        (FileDescriptor {
            message_and_enums: message_and_enums
        })));

    #[test]
    fn test_message() {
        let msg = r#"message ReferenceData 
    {
        repeated ScenarioInfo  scenarioSet = 1;
        repeated CalculatedObjectInfo calculatedObjectSet = 2;  
        repeated RiskFactorList riskFactorListSet = 3;
        repeated RiskMaturityInfo riskMaturitySet = 4;
        repeated IndicatorInfo indicatorSet = 5;
        repeated RiskStrikeInfo riskStrikeSet = 6;
        repeated FreeProjectionList freeProjectionListSet = 7;
        repeated ValidationProperty ValidationSet = 8;
        repeated CalcProperties calcPropertiesSet = 9;
        repeated MaturityInfo maturitySet = 10;
    }"#;

        let mess = message(msg.as_bytes());
        if let ::nom::IResult::Done(_, mess) = mess {
            assert_eq!(10, mess.fields.len());
        }
    }

    #[test]
    fn test_enum() {
        let msg = r#"enum PairingStatus {
                DEALPAIRED        = 0;
                INVENTORYORPHAN   = 1;
                CALCULATEDORPHAN  = 2;
                CANCELED          = 3;
    }"#;

        let mess = enumerator(msg.as_bytes());
        if let ::nom::IResult::Done(_, mess) = mess {
            assert_eq!(4, mess.fields.len());
        }
    }

    #[test]
    fn test_ignore() {
        let msg = r#"package com.test.v0;

    option optimize_for = SPEED;
    "#;

        match ignore(msg.as_bytes()) {
            ::nom::IResult::Done(_, _) => (),
            e => panic!("Expecting done {:?}", e),
        }
    }
}