//! A derive-macro which produces code to access signals within CAN
//! messages, as described by a `.dbc` file.  The generated code has
//! very few dependencies: just core primitives and `[u8]` slices, and
//! is `#[no_std]` compatible.
//!
//! # Changelog
//! [CHANGELOG.md]
//!
//! # Example
//! Given a `.dbc` file containing:
//!
//! ```text
//! BO_ 1023 SomeMessage: 4 Ecu1
//!  SG_ Unsigned16 : 16|16@0+ (1,0) [0|0] "" Vector__XXX
//!  SG_ Unsigned8 : 8|8@1+ (1,0) [0|0] "" Vector__XXX
//!  SG_ Signed8 : 0|8@1- (1,0) [0|0] "" Vector__XXX
//! ```
//! The following code can be written to access the fields of the
//! message:
//!
//! ```
//! pub use dbc_data::*;
//!
//! #[derive(DbcData, Default)]
//! #[dbc_file = "tests/example.dbc"]
//! struct TestData {
//!     some_message: SomeMessage,
//! }
//!
//! fn test() {
//!     let mut t = TestData::default();
//!
//!     assert_eq!(SomeMessage::ID, 1023);
//!     assert_eq!(SomeMessage::DLC, 4);
//!     assert!(t.some_message.decode(&[0xFE, 0x34, 0x56, 0x78]));
//!     assert_eq!(t.some_message.signed8, -2);
//!     assert_eq!(t.some_message.unsigned8, 0x34);
//!     assert_eq!(t.some_message.unsigned16, 0x5678); // big-endian
//! }
//! ```
//! See the test cases in this crate for examples of usage.
//!
//! # Code Generation
//! This crate is aimed at embedded systems where typically some
//! subset of the messages and signals defined in the `.dbc` file are
//! of interest, and the rest can be ignored for a minimal footpint.
//! If you need to decode the entire DBC into rich (possibly `std`-dependent)
//! types to run on a host system, there are other crates for that
//! such as `dbc_codegen`.
//!
//! ## Messages
//! As `.dbc` files typically contain multiple messages, each of these
//! can be brought into scope by referencing their name as a type
//! (e.g. `SomeMessage` as shown above) and this determines what code
//! is generated.  Messages not referenced will not generate any code.
//!
//! When a range of message IDs contain the same signals, such as a
//! series of readings which do not fit into a single message, then
//! declaring an array will allow that type to be used for all of them.
//!
//! # Signals
//! For cases where only certain signals within a message are needed, the
//! `#[dbc_signals]` attribute lets you specify which ones are used.
//!
//! ## Types
//! Single-bit signals generate `bool` types, and signals with a scale factor
//! generate `f32` types.  All other signals generate signed or unsigned
//! native types which are large enough to fit the contained values, e.g.
//! 13-bit signals will be stored in a `u16` and 17-bit signals will be
//! stored in a `u32`.
//!
//! # Functionality
//! * Decode signals from PDU into native types
//!     * const definitions for `ID: u32`, `DLC: u8`, `EXTENDED: bool`,
//!       and `CYCLE_TIME: usize` when present
//! * Encode signals into PDU (all alignments and byte orders)
//!
//! # TODO
//! * Generate dispatcher for decoding based on ID (including ranges)
//! * Enforce that arrays of messages contain the same signals
//! * Support multiplexed signals
//! * Emit `enum`s for value-tables, with optional type association (basic VAL_TABLE_ support done)
//! * (Maybe) scope generated types to a module
//!
//! # License
//! [LICENSE-MIT]
//!

extern crate proc_macro;
use can_dbc::{
    AttributeValuedForObjectType, ByteOrder, DBC, MessageId, Signal, ValueType,
};
use proc_macro2::TokenStream;
use quote::{TokenStreamExt, quote};
use std::{collections::BTreeMap, fs::read};
use syn::{
    Attribute, Data, DeriveInput, Expr, Field, Fields, Ident, Lit, Meta,
    Result, Type, parse_macro_input, spanned::Spanned,
};

/// Convert a string to snake_case.
///
/// Handles PascalCase, camelCase, SCREAMING_SNAKE_CASE, and mixtures:
///   "VCU0Tx1TCU0" → "vcu0_tx1_tcu0"
///   "ROBOT_COMMAND" → "robot_command"
///   "StatusIbxFlow" → "status_ibx_flow"
///   "already_snake" → "already_snake"
fn to_snake_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    let mut prev_was_upper = false;
    let mut prev_was_underscore = true; // treat start as boundary

    for (i, ch) in s.chars().enumerate() {
        if ch == '_' {
            if !out.is_empty() && !prev_was_underscore {
                out.push('_');
            }
            prev_was_upper = false;
            prev_was_underscore = true;
            continue;
        }

        if ch.is_uppercase() {
            let next_is_lower = s[i + ch.len_utf8()..]
                .chars()
                .next()
                .is_some_and(|c| c.is_lowercase());

            // Insert underscore before:
            //  - a capital that starts a new word (preceded by lowercase)
            //  - a capital in a run of capitals followed by lowercase (e.g. "TCU0")
            if !prev_was_underscore
                && ((!prev_was_upper) || (prev_was_upper && next_is_lower))
            {
                out.push('_');
            }

            out.push(ch.to_lowercase().next().unwrap());
            prev_was_upper = true;
        } else {
            out.push(ch);
            prev_was_upper = false;
        }
        prev_was_underscore = false;
    }
    out
}

/// Normalize DBC file content to work around can-dbc parser quirks:
/// - "BS_ :" → "BS_:" (parser requires no space before colon)
/// - Collapse blank lines between BO_ blocks (parser may fail on them)
/// - Normalize SG_ indentation to single space
/// - Ensure trailing newline
fn normalize_dbc(input: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut prev_was_empty = false;

    for line in input.lines() {
        let trimmed = line.trim();

        // Fix "BS_ :" → "BS_:"
        if trimmed.starts_with("BS_") && trimmed.contains(':') {
            lines.push(trimmed.replace("BS_ :", "BS_:"));
            prev_was_empty = false;
            continue;
        }

        // Skip blank lines between message blocks to avoid parser issues
        if trimmed.is_empty() {
            // Only keep blank lines before BO_ sections or after header sections
            prev_was_empty = true;
            continue;
        }

        // Re-insert a single blank line before structural sections
        if prev_was_empty
            && (trimmed.starts_with("BO_")
                || trimmed.starts_with("BU_")
                || trimmed.starts_with("CM_")
                || trimmed.starts_with("BA_")
                || trimmed.starts_with("VAL_"))
        {
            lines.push(String::new());
        }

        // Normalize SG_ indentation to single space
        if trimmed.starts_with("SG_") {
            lines.push(format!(" {trimmed}"));
        } else {
            lines.push(trimmed.to_string());
        }

        prev_was_empty = false;
    }

    // Ensure trailing newline
    let mut result = lines.join("\n");
    if !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

struct DeriveData<'a> {
    /// Name of the struct we are deriving for
    #[allow(dead_code)]
    name: &'a Ident,
    /// The parsed DBC file
    dbc: can_dbc::DBC,
    /// All of the messages to derive
    messages: BTreeMap<String, MessageInfo<'a>>,
}

struct MessageInfo<'a> {
    id: u32,
    extended: bool,
    index: usize,
    ident: &'a Ident,
    attrs: &'a Vec<Attribute>,
    cycle_time: Option<usize>,
}

/// Filter signals based on #[dbc_signals] list
struct SignalFilter {
    names: Vec<String>,
}

impl SignalFilter {
    /// Create a signal filter from a message's attribute
    fn new(message: &MessageInfo) -> Self {
        let mut names: Vec<String> = vec![];
        if let Some(attrs) = parse_attr(message.attrs, "dbc_signals") {
            let list = attrs.split(",");
            for name in list {
                let name = name.trim();
                names.push(name.to_string());
            }
        }
        Self { names }
    }

    /// Return whether a signal should be used, i.e. whether it is
    /// in the filter list or the list is empty
    fn use_signal(&self, name: impl Into<String>) -> bool {
        if self.names.is_empty() {
            return true;
        }
        let name = name.into();
        self.names.contains(&name)
    }
}

/// Information about signal within message
struct SignalInfo<'a> {
    signal: &'a Signal,
    ident: Ident,
    ntype: Ident,
    utype: Ident,
    start: usize,
    width: usize,
    nwidth: usize,
    scale: f32,
    signed: bool,
}

impl<'a> SignalInfo<'a> {
    fn new(signal: &'a Signal, message: &MessageInfo) -> Self {
        let name = to_snake_case(signal.name());
        let signed = matches!(signal.value_type(), ValueType::Signed);
        let width = *signal.signal_size() as usize;
        let scale = *signal.factor() as f32;

        // get storage width of signal data
        let nwidth = match width {
            1 => 1,
            2..=8 => 8,
            9..=16 => 16,
            17..=32 => 32,
            _ => 64,
        };

        let utype = if width == 1 {
            "bool"
        } else {
            &format!("{}{}", if signed { "i" } else { "u" }, nwidth)
        };

        // get native type for signal
        let ntype = if scale == 1.0 { utype } else { "f32" };

        Self {
            signal,
            ident: Ident::new(&name, message.ident.span()),
            ntype: Ident::new(ntype, message.ident.span()),
            utype: Ident::new(utype, message.ident.span()),
            start: *signal.start_bit() as usize,
            scale,
            signed,
            width,
            nwidth,
        }
    }

    /// Generate the code for extracting signal bits using an
    /// accumulator pattern that handles all alignment cases uniformly.
    fn extract_bits(&self) -> TokenStream {
        let utype = &self.utype;
        let le = self.signal.byte_order() == &ByteOrder::LittleEndian;
        let width = self.width;

        let mut ts = TokenStream::new();

        if le {
            let start_byte = self.start / 8;
            let s_off = self.start % 8;
            let end_bit = self.start + width - 1;
            let end_byte = end_bit / 8;
            let e_off = end_bit % 8;

            // Accumulate masked bytes into u64
            ts.append_all(quote! { let mut acc: u64 = 0; });
            for b in start_byte..=end_byte {
                let idx = b - start_byte;
                if start_byte == end_byte {
                    // single byte: mask both ends
                    let mask = ((0xFFu16 << s_off as u16)
                        & (0xFFu16 >> (7 - e_off as u16)))
                        as u8;
                    ts.append_all(quote! {
                        acc |= ((pdu[#b] & #mask) as u64) << (8 * #idx);
                    });
                } else if b == start_byte {
                    let mask = (0xFFu16 << s_off as u16) as u8;
                    ts.append_all(quote! {
                        acc |= ((pdu[#b] & #mask) as u64) << (8 * #idx);
                    });
                } else if b == end_byte {
                    let mask = (0xFFu16 >> (7 - e_off as u16)) as u8;
                    ts.append_all(quote! {
                        acc |= ((pdu[#b] & #mask) as u64) << (8 * #idx);
                    });
                } else {
                    ts.append_all(quote! {
                        acc |= (pdu[#b] as u64) << (8 * #idx);
                    });
                }
            }
            // Shift and mask to extract the value
            let mask_expr = if width == 64 {
                quote! { u64::MAX }
            } else {
                quote! { ((1u64 << #width) - 1) }
            };
            ts.append_all(quote! {
                let v = ((acc >> #s_off) & #mask_expr) as #utype;
            });
        } else {
            // Big-endian (Motorola): per-bit gather, MSB first.
            // In DBC format, start_bit for BE is the Motorola bit
            // position of the MSB: byte = sb/8, bit_in_byte = sb%8.
            // Walk: bit-1, wrapping from 0 to 7 of the next byte.
            let sb = self.start;
            ts.append_all(quote! {
                let mut raw: u64 = 0;
                {
                    let mut byte = (#sb / 8) as usize;
                    let mut bit = (#sb % 8) as i32;
                    let mut i = 0usize;
                    while i < #width {
                        let b = ((pdu[byte] >> (bit as u8)) & 1) as u64;
                        raw |= b << (#width - 1 - i);
                        bit -= 1;
                        if bit < 0 { bit = 7; byte += 1; }
                        i += 1;
                    }
                }
            });
            ts.append_all(quote! {
                let v = raw as #utype;
            });
        }

        // Sign-extend for signed values with fewer bits than storage
        if self.signed && self.width < self.nwidth {
            let width = self.width;
            ts.append_all(quote! {
                let v = (((v as i64) << (64 - #width)) >> (64 - #width)) as #utype;
            });
        }

        ts.append_all(quote! { v });
        quote! { { #ts } }
    }

    fn gen_decoder(&self) -> TokenStream {
        let name = &self.ident;
        if self.width == 1 {
            // boolean
            let byte = self.start / 8;
            let bit = self.start % 8;
            quote! {
                self.#name = (pdu[#byte] & (1 << #bit)) != 0;
            }
        } else {
            let value = self.extract_bits();
            let ntype = &self.ntype;
            if !self.is_float() {
                quote! {
                    self.#name = #value as #ntype;
                }
            } else {
                let scale = self.scale;
                let offset = *self.signal.offset() as f32;
                quote! {
                    self.#name = (#value as f32) * #scale + #offset;
                }
            }
        }
    }

    fn gen_encoder(&self) -> TokenStream {
        let name = &self.ident;
        let bit = self.start % 8;
        let width = self.width;

        if width == 1 {
            // boolean
            let byte = self.start / 8;
            return quote! {
                let mask: u8 = (1 << #bit);
                if self.#name {
                    pdu[#byte] |= mask;
                } else {
                    pdu[#byte] &= !mask;
                }
            };
        }

        let utype = &self.utype;
        let le = self.signal.byte_order() == &ByteOrder::LittleEndian;

        let mut ts = TokenStream::new();
        if self.is_float() {
            let scale = self.scale;
            let offset = self.signal.offset as f32;
            ts.append_all(quote! {
                let v = (((self.#name - #offset) * (1.0 / #scale)).round()) as #utype;
            });
        } else {
            ts.append_all(quote! {
                let v = self.#name;
            });
        }

        if le {
            let start_byte = self.start / 8;
            let s_off = self.start % 8;
            let end_bit = self.start + width - 1;
            let end_byte = end_bit / 8;
            let e_off = end_bit % 8;

            // Mask value to signal width and shift into position
            let mask_expr = if width == 64 {
                quote! { u64::MAX }
            } else {
                quote! { ((1u64 << #width) - 1) }
            };
            if self.signed {
                ts.append_all(quote! {
                    let mut val: u64 = (((v as i64) as i128 & ((1i128 << #width) - 1)) as u64);
                });
            } else {
                ts.append_all(quote! {
                    let mut val: u64 = (v as u64) & #mask_expr;
                });
            }
            ts.append_all(quote! {
                val <<= #s_off;
            });

            // Write each byte with proper masking
            for b in start_byte..=end_byte {
                let idx = b - start_byte;
                if start_byte == end_byte {
                    let mask = ((0xFFu16 << s_off as u16)
                        & (0xFFu16 >> (7 - e_off as u16)))
                        as u8;
                    ts.append_all(quote! {
                        pdu[#b] = (pdu[#b] & !#mask)
                            | ((((val >> (8 * #idx)) & 0xFF) as u8) & #mask);
                    });
                } else if b == start_byte {
                    let mask = (0xFFu16 << s_off as u16) as u8;
                    ts.append_all(quote! {
                        pdu[#b] = (pdu[#b] & !#mask)
                            | (((val >> (8 * #idx)) & 0xFF) as u8 & #mask);
                    });
                } else if b == end_byte {
                    let mask = (0xFFu16 >> (7 - e_off as u16)) as u8;
                    ts.append_all(quote! {
                        pdu[#b] = (pdu[#b] & !#mask)
                            | (((val >> (8 * #idx)) & 0xFF) as u8 & #mask);
                    });
                } else {
                    ts.append_all(quote! {
                        pdu[#b] = ((val >> (8 * #idx)) & 0xFF) as u8;
                    });
                }
            }
        } else {
            // Big-endian (Motorola): per-bit scatter, MSB first.
            let sb = self.start;
            let mask_expr = if width == 64 {
                quote! { u64::MAX }
            } else {
                quote! { ((1u64 << #width) - 1) }
            };
            if self.signed {
                ts.append_all(quote! {
                    let val: u64 = (((v as i64) as i128 & ((1i128 << #width) - 1)) as u64);
                });
            } else {
                ts.append_all(quote! {
                    let val: u64 = (v as u64) & #mask_expr;
                });
            }
            ts.append_all(quote! {
                {
                    let mut byte = (#sb / 8) as usize;
                    let mut bit = (#sb % 8) as i32;
                    let mut i = 0usize;
                    while i < #width {
                        let src = ((val >> (#width - 1 - i)) & 1) as u8;
                        if src == 1 {
                            pdu[byte] |= 1u8 << (bit as u8);
                        } else {
                            pdu[byte] &= !(1u8 << (bit as u8));
                        }
                        bit -= 1;
                        if bit < 0 { bit = 7; byte += 1; }
                        i += 1;
                    }
                }
            });
        }
        ts
    }

    fn is_float(&self) -> bool {
        self.scale != 1.0
    }
}

impl<'a> MessageInfo<'a> {
    fn new(dbc: &DBC, field: &'a Field) -> Option<Self> {
        let stype = match &field.ty {
            Type::Path(v) => v,
            Type::Array(a) => match *a.elem {
                // TODO: validate that all signals match in ID range
                Type::Path(ref v) => v,
                _ => unimplemented!(),
            },
            _ => unimplemented!(),
        };
        let ident = &stype.path.segments[0].ident;
        let name = to_snake_case(&ident.to_string());

        for (index, message) in dbc.messages().iter().enumerate() {
            if to_snake_case(message.message_name()) == name {
                let id = message.message_id();
                let (id32, extended) = match *id {
                    MessageId::Standard(id) => (id as u32, false),
                    MessageId::Extended(id) => (id, true),
                };
                let mut cycle_time: Option<usize> = None;
                for attr in dbc.attribute_values().iter() {
                    let value = attr.attribute_value();
                    use AttributeValuedForObjectType as AV;
                    match value {
                        AV::MessageDefinitionAttributeValue(aid, Some(av)) => {
                            if aid == id
                                && attr.attribute_name() == "GenMsgCycleTime"
                            {
                                cycle_time = Some(Self::attr_value(av));
                            }
                        }
                        _ => {}
                    }
                }

                return Some(Self {
                    id: id32,
                    extended,
                    index,
                    ident,
                    cycle_time,
                    attrs: &field.attrs,
                });
            }
        }
        None
    }

    // TODO: revisit this to handle type conversion better; we
    // expect that the value fits in a usize for e.g. GenMsgCycleTime
    fn attr_value(v: &can_dbc::AttributeValue) -> usize {
        use can_dbc::AttributeValue as AV;
        match v {
            AV::AttributeValueU64(x) => *x as usize,
            AV::AttributeValueI64(x) => *x as usize,
            AV::AttributeValueF64(x) => *x as usize,
            AV::AttributeValueCharString(_) => 0usize, // TODO: parse as int?
        }
    }
}

impl<'a> DeriveData<'a> {
    fn from(input: &'a DeriveInput) -> Result<Self> {
        // load the DBC file
        let dbc_file = parse_attr(&input.attrs, "dbc_file")
            .expect("No DBC file specified");
        let contents = read(&dbc_file).expect("Could not read DBC");
        let contents = normalize_dbc(&String::from_utf8_lossy(&contents));
        let dbc = match DBC::from_slice(contents.as_bytes()) {
            Ok(dbc) => dbc,
            Err(can_dbc::Error::Incomplete(dbc, _)) => {
                // TODO: emit an actual compiler warning
                eprintln!(
                    "Warning: DBC load incomplete; some data may be missing"
                );
                dbc
            }
            Err(_) => {
                panic!("Unable to parse {dbc_file}");
            }
        };

        // Collect VAL_TABLE_ names so we can accept them as fields
        let val_table_names: std::collections::HashSet<String> = dbc
            .value_tables()
            .iter()
            .map(|vt| vt.value_table_name().clone())
            .collect();

        // gather all of the messages and associated attributes
        let mut messages: BTreeMap<String, MessageInfo<'_>> =
            Default::default();
        match &input.data {
            Data::Struct(data) => match &data.fields {
                Fields::Named(fields) => {
                    for field in &fields.named {
                        if let Some(info) = MessageInfo::new(&dbc, field) {
                            messages.insert(info.ident.to_string(), info);
                        } else if Self::field_type_name(field)
                            .map_or(false, |n| val_table_names.contains(&n))
                        {
                            // Field type matches a VAL_TABLE_ enum;
                            // skip it (the enum is generated separately)
                        } else {
                            return Err(syn::Error::new(
                                field.span(),
                                "Unknown message",
                            ));
                        }
                    }
                }
                Fields::Unnamed(_) | Fields::Unit => unimplemented!(),
            },
            _ => unimplemented!(),
        }

        Ok(Self {
            name: &input.ident,
            dbc,
            messages,
        })
    }

    /// Extract the type name from a struct field (e.g. `foo: MyType` → "MyType")
    fn field_type_name(field: &Field) -> Option<String> {
        match &field.ty {
            Type::Path(v) => Some(v.path.segments[0].ident.to_string()),
            _ => None,
        }
    }

    fn build(self) -> TokenStream {
        let mut out = TokenStream::new();

        // Generate enums from VAL_TABLE_ definitions
        let mut generated_enums = std::collections::HashSet::new();
        for vt in self.dbc.value_tables().iter() {
            let table_name = vt.value_table_name();
            if table_name.is_empty() || vt.value_descriptions().is_empty() {
                continue;
            }
            if !generated_enums.insert(table_name.clone()) {
                continue; // skip duplicates
            }
            out.append_all(Self::gen_enum(table_name, vt.value_descriptions()));
        }

        for (name, message) in self.messages.iter() {
            let m = self
                .dbc
                .messages()
                .get(message.index)
                .unwrap_or_else(|| panic!("Unknown message {name}"));

            let filter = SignalFilter::new(message);

            let mut signals: Vec<Ident> = vec![];
            let mut types: Vec<Ident> = vec![];
            let mut infos: Vec<SignalInfo> = vec![];
            for s in m.signals().iter() {
                if !filter.use_signal(to_snake_case(s.name())) {
                    continue;
                }

                let signal = SignalInfo::new(s, message);
                signals.push(signal.ident.clone());
                types.push(signal.ntype.clone());
                infos.push(signal);
            }

            let id = message.id;
            let extended = message.extended;

            let dlc = *m.message_size() as usize;
            let dlc8 = dlc as u8;
            let ident = message.ident;

            // build signal decoders and encoders
            let mut decoders = TokenStream::new();
            let mut encoders = TokenStream::new();
            for info in infos.iter() {
                decoders.append_all(info.gen_decoder());
                encoders.append_all(info.gen_encoder());
            }
            let cycle_time = if let Some(c) = message.cycle_time {
                quote! {
                    const CYCLE_TIME: usize = #c;
                }
            } else {
                quote! {}
            };

            out.append_all(quote! {
                #[allow(dead_code)]
                #[allow(non_snake_case)]
                #[allow(non_camel_case_types)]
                #[derive(Default)]
                pub struct #ident {
                    #(
                        pub #signals: #types
                    ),*
                }

                impl #ident {
                    pub const ID: u32 = #id;
                    pub const DLC: u8 = #dlc8;
                    pub const EXTENDED: bool = #extended;
                    #cycle_time

                    pub fn decode(&mut self, pdu: &[u8])
                                  -> bool {
                        if pdu.len() != #dlc {
                            return false
                        }
                        #decoders
                        true
                    }

                    pub fn encode(&mut self, pdu: &mut [u8])
                                  -> bool {
                        if pdu.len() != #dlc {
                            return false
                        }
                        #encoders
                        true
                    }
                }
            });
        }
        out
    }

    /// Generate a Rust enum from a DBC VAL_TABLE_ definition.
    ///
    /// Produces:
    /// - `#[repr(u8)]` enum with `Debug, Clone, Copy, PartialEq, Eq`
    /// - `TryFrom<u8>` impl
    /// - `From<Enum> for u8` impl
    /// - `Default` impl (variant with value 0, or lowest value)
    fn gen_enum(
        name: &str,
        descriptions: &[can_dbc::ValDescription],
    ) -> TokenStream {
        // Sort by numeric value for deterministic output
        let mut entries: Vec<(u8, String)> = descriptions
            .iter()
            .map(|vd| (*vd.a() as u8, vd.b().clone()))
            .collect();
        entries.sort_by_key(|(v, _)| *v);

        if entries.is_empty() {
            return TokenStream::new();
        }

        let enum_ident = Ident::new(name, proc_macro2::Span::call_site());

        let variant_idents: Vec<Ident> = entries
            .iter()
            .map(|(_, desc)| Ident::new(desc, proc_macro2::Span::call_site()))
            .collect();
        let variant_values: Vec<u8> = entries.iter().map(|(v, _)| *v).collect();

        // Default to variant with value 0 if it exists, otherwise
        // the first (lowest-valued) variant
        let default_ident = entries
            .iter()
            .find(|(v, _)| *v == 0)
            .map(|(_, desc)| desc.as_str())
            .unwrap_or(&entries[0].1);
        let default_ident =
            Ident::new(default_ident, proc_macro2::Span::call_site());

        let error_msg = format!("Invalid {} value", name);

        quote! {
            #[repr(u8)]
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            #[allow(dead_code)]
            #[allow(non_camel_case_types)]
            pub enum #enum_ident {
                #(
                    #variant_idents = #variant_values
                ),*
            }

            impl core::convert::TryFrom<u8> for #enum_ident {
                type Error = &'static str;

                fn try_from(value: u8) -> Result<Self, Self::Error> {
                    match value {
                        #(
                            #variant_values => Ok(Self::#variant_idents),
                        )*
                        _ => Err(#error_msg),
                    }
                }
            }

            impl From<#enum_ident> for u8 {
                fn from(value: #enum_ident) -> u8 {
                    value as u8
                }
            }

            impl Default for #enum_ident {
                fn default() -> Self {
                    Self::#default_ident
                }
            }
        }
    }
}

#[proc_macro_derive(DbcData, attributes(dbc_file, dbc_signals))]
pub fn dbc_data_derive(
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    derive_data(&parse_macro_input!(input as DeriveInput))
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}

fn derive_data(input: &DeriveInput) -> Result<TokenStream> {
    Ok(DeriveData::from(input)?.build())
}

fn parse_attr(attrs: &[Attribute], name: &str) -> Option<String> {
    let attr = attrs
        .iter()
        .filter(|a| {
            a.path().segments.len() == 1 && a.path().segments[0].ident == name
        })
        .nth(0)?;

    let expr = match &attr.meta {
        Meta::NameValue(n) => Some(&n.value),
        _ => None,
    };

    match &expr {
        Some(Expr::Lit(e)) => match &e.lit {
            Lit::Str(s) => Some(s.value()),
            _ => None,
        },
        _ => None,
    }
}
