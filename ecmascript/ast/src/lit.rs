use crate::jsx::JSXText;
use num_bigint::BigInt as BigIntValue;
use serde::{Deserialize, Serialize};
use std::{
    fmt::{self, Display, Formatter},
    hash::{Hash, Hasher},
    mem,
};
use swc_atoms::JsWord;
use swc_common::{ast_node, EqIgnoreSpan, Span};

#[ast_node]
#[derive(Eq, Hash, EqIgnoreSpan)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub enum Lit {
    #[tag("StringLiteral")]
    Str(Str),

    #[tag("BooleanLiteral")]
    Bool(Bool),

    #[tag("NullLiteral")]
    Null(Null),

    #[tag("NumericLiteral")]
    Num(Number),

    #[tag("BigIntLiteral")]
    BigInt(BigInt),

    #[tag("RegExpLiteral")]
    Regex(Regex),

    #[tag("JSXText")]
    JSXText(JSXText),
}

#[ast_node("BigIntLiteral")]
#[derive(Eq, Hash, EqIgnoreSpan)]
pub struct BigInt {
    pub span: Span,
    pub value: BigIntValue,
}

#[cfg(feature = "arbitrary")]
impl<'a> arbitrary::Arbitrary<'a> for BigInt {
    fn arbitrary(u: &mut arbitrary::Unstructured<'_>) -> arbitrary::Result<Self> {
        let span = u.arbitrary()?;
        let value = u.arbitrary::<usize>()?.into();

        Ok(Self { span, value })
    }
}

#[ast_node("StringLiteral")]
#[derive(Eq, Hash, EqIgnoreSpan)]
pub struct Str {
    pub span: Span,

    pub value: JsWord,

    /// This includes line escape.
    #[serde(default)]
    pub has_escape: bool,

    #[serde(default)]
    pub kind: StrKind,
}

/// THis enum determines how string literal should be printed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StrKind {
    /// Span of string points to original source code, and codegen should use
    /// it.
    //
    /// **Note**: Giving wrong value to this field will result in invalid
    /// codegen.
    #[serde(rename = "normal")]
    Normal {
        /// Does span of this string literal contains quote?
        ///
        /// True for string literals generated by parser, false for string
        /// literals generated by various passes.
        #[serde(rename = "containsQuote")]
        contains_quote: bool,
    },
    /// If the span of string does not point a string literal, mainly because
    /// this string is synthesized, this variant should be used.
    #[serde(rename = "synthesized")]
    Synthesized,
}

/// Always returns true as this is not a data of a string literal.
impl EqIgnoreSpan for StrKind {
    fn eq_ignore_span(&self, _: &Self) -> bool {
        true
    }
}

impl Default for StrKind {
    fn default() -> Self {
        Self::Synthesized
    }
}

#[cfg(feature = "arbitrary")]
impl<'a> arbitrary::Arbitrary<'a> for Str {
    fn arbitrary(u: &mut arbitrary::Unstructured<'_>) -> arbitrary::Result<Self> {
        let span = u.arbitrary()?;
        let value = u.arbitrary::<String>()?.into();

        Ok(Self {
            span,
            value,
            has_escape: false,
            kind: Default::default(),
        })
    }
}

impl Str {
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }
}

#[ast_node("BooleanLiteral")]
#[derive(Copy, Eq, Hash, EqIgnoreSpan)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct Bool {
    pub span: Span,
    pub value: bool,
}

#[ast_node("NullLiteral")]
#[derive(Copy, Eq, Hash, EqIgnoreSpan)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct Null {
    pub span: Span,
}

#[ast_node("RegExpLiteral")]
#[derive(Eq, Hash, EqIgnoreSpan)]
pub struct Regex {
    pub span: Span,

    #[serde(rename = "pattern")]
    pub exp: JsWord,

    #[serde(default)]
    pub flags: JsWord,
}

#[cfg(feature = "arbitrary")]
impl<'a> arbitrary::Arbitrary<'a> for Regex {
    fn arbitrary(u: &mut arbitrary::Unstructured<'_>) -> arbitrary::Result<Self> {
        let span = u.arbitrary()?;
        let exp = u.arbitrary::<String>()?.into();
        let flags = "".into(); // TODO

        Ok(Self { span, exp, flags })
    }
}

#[ast_node("NumericLiteral")]
#[derive(Copy, EqIgnoreSpan)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct Number {
    pub span: Span,
    /// **Note**: This should not be `NaN`. Use [crate::Ident] to represent NaN.
    ///
    /// If you store `NaN` in this field, a hash map will behave strangely.
    #[use_eq]
    pub value: f64,
}

impl Eq for Number {}

impl Hash for Number {
    fn hash<H: Hasher>(&self, state: &mut H) {
        fn integer_decode(val: f64) -> (u64, i16, i8) {
            let bits: u64 = unsafe { mem::transmute(val) };
            let sign: i8 = if bits >> 63 == 0 { 1 } else { -1 };
            let mut exponent: i16 = ((bits >> 52) & 0x7ff) as i16;
            let mantissa = if exponent == 0 {
                (bits & 0xfffffffffffff) << 1
            } else {
                (bits & 0xfffffffffffff) | 0x10000000000000
            };

            exponent -= 1023 + 52;
            (mantissa, exponent, sign)
        }

        self.span.hash(state);
        integer_decode(self.value).hash(state);
    }
}

impl Display for Number {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.value.is_infinite() {
            if self.value.is_sign_positive() {
                Display::fmt("Infinity", f)
            } else {
                Display::fmt("-Infinity", f)
            }
        } else {
            Display::fmt(&self.value, f)
        }
    }
}
