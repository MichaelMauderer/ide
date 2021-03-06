#![feature(associated_type_bounds)]
#![feature(bool_to_option)]
#![feature(generators, generator_trait)]
#![feature(trivial_bounds)]
#![feature(type_alias_impl_trait)]

#[warn(missing_docs)]
pub mod assoc;
#[warn(missing_docs)]
pub mod internal;
#[warn(missing_docs)]
pub mod known;
#[warn(missing_docs)]
pub mod opr;
#[warn(missing_docs)]
pub mod prefix;
#[warn(missing_docs)]
pub mod repr;
#[warn(missing_docs)]
pub mod test_utils;

use prelude::*;

use ast_macros::*;
use data::text::*;

use serde::de::Deserializer;
use serde::de::Visitor;
use serde::Deserialize;
use serde::ser::Serializer;
use serde::ser::SerializeStruct;
use serde::Serialize;
use shapely::*;
use uuid::Uuid;



#[derive(Clone,Debug,Default,Deserialize,Eq,PartialEq,Serialize)]
pub struct IdMap(pub Vec<(Span,ID)>);

impl IdMap {
    pub fn insert(&mut self, span:Span, id:ID) {
        self.0.push((span, id));
    }
}

/// A sequence of AST nodes, typically the "token soup".
pub type Stream<T> = Vec<T>;



// ==============
// === Errors ===
// ==============

/// Exception raised by macro-generated TryFrom methods that try to "downcast"
/// enum type to its variant subtype if different constructor was used.
#[derive(Display, Debug, Fail)]
pub struct WrongEnum { pub expected_con: String }



// ============
// === Tree ===
// ============

/// A tree structure where each node may store value of `K` and has arbitrary
/// number of children nodes, each marked with a single `K`.
///
/// It is used to describe ambiguous macro match.
#[derive(Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct Tree<K,V> {
    pub value    : Option<V>,
    pub branches : Vec<(K, Tree<K,V>)>,
}



// ===============
// === Shifted ===
// ===============

/// A value of type `T` annotated with offset value `off`.
#[derive(Eq, PartialEq, Debug, Serialize, Deserialize, Shrinkwrap, Iterator)]
#[shrinkwrap(mutable)]
pub struct Shifted<T> {
    #[shrinkwrap(main_field)]
    pub wrapped : T,
    pub off     : usize,
}

/// A non-empty sequence of `T`s interspersed by offsets.
#[derive(Eq, PartialEq, Debug, Serialize, Deserialize, Iterator)]
pub struct ShiftedVec1<T> {
    pub head: T,
    pub tail: Vec<Shifted<T>>
}



// =============
// === Layer ===
// =============

// === Trait ===

/// Types that can wrap a value of given `T`.
///
/// Same API as `From`, however not reflexive.
pub trait Layer<T> {
    fn layered(t: T) -> Self;
}

impl<T> From<T> for Layered<T> {
    fn from(t: T) -> Self {  Layered::layered(t) }
}


// === Layered ===

/// A trivial `Layer` type that is just a strongly typed wrapper over `T`.
#[derive(Debug)]
#[derive(Shrinkwrap)]
#[shrinkwrap(mutable)]
pub struct Layered<T>(pub T);

impl<T> Layer<T> for Layered<T> {
    fn layered(t: T) -> Self { Layered(t) }
}



// ============
// === Unit ===
// ============

/// A unit type defined as an empty struct.
///
/// Because it is defined using {} syntax, serde_json will serialize it to
/// an empty object rather than null node. This is to workaround issue with
/// using units in `Option`, reported here:
/// https://github.com/serde-rs/serde/issues/1690
#[ast_node] pub struct Unit{}



// ===========
// === AST ===
// ===========

/// The primary class for Enso Abstract Syntax Tree.
///
/// This implementation is paired with AST implementation for Scala. Any changes
/// to either of the implementation need to be applied to the other one as well.
///
/// Each AST node is annotated with span and an optional ID.
#[derive(Eq, PartialEq, Debug, Shrinkwrap)]
#[shrinkwrap(mutable)]
pub struct Ast {
    pub wrapped: Rc<WithID<WithLength<Shape<Ast>>>>
}

impl Clone for Ast {
    fn clone(&self) -> Self {
        Ast { wrapped: self.wrapped.clone() }
    }
}

/// `IntoIterator` for `&Ast` that just delegates to `&Shape`'s `IntoIterator`.
impl<'t> IntoIterator for &'t Ast {
    type Item     = <&'t Shape<Ast> as IntoIterator>::Item;
    type IntoIter = <&'t Shape<Ast> as IntoIterator>::IntoIter;
    fn into_iter(self) -> Self::IntoIter {
        self.shape().into_iter()
    }
}

impl Ast {
    pub fn shape(&self) -> &Shape<Ast> {
        self
    }

    /// Wraps given shape with an optional ID into Ast.
    /// Length will ba automatically calculated based on Shape.
    pub fn new<S:Into<Shape<Ast>>>(shape:S, id:Option<ID>) -> Ast {
        let shape: Shape<Ast> = shape.into();
        let length = shape.len();
        Ast::new_with_length(shape,id,length)
    }

    /// Just wraps shape, id and len into Ast node.
    pub fn from_ast_id_len(shape:Shape<Ast>, id:Option<ID>, len:usize) -> Ast {
        let with_length = WithLength { wrapped:shape      , len };
        let with_id     = WithID     { wrapped:with_length, id  };
        Ast { wrapped: Rc::new(with_id) }
    }

    /// As `new` but sets given declared length for the shape.
    pub fn new_with_length<S:Into<Shape<Ast>>>
    (shape:S, id:Option<ID>, len:usize) -> Ast {
        let shape = shape.into();
        Self::from_ast_id_len(shape,id,len)
    }

    /// Iterates over all transitive child nodes (including self).
    pub fn iter_recursive(&self) -> impl Iterator<Item=&Ast> {
        internal::iterate_subtree(self)
    }
}

/// Fills `id` with `None` by default.
impl<T:Into<Shape<Ast>>>
From<T> for Ast {
    fn from(t:T) -> Self {
        let id = None;
        Ast::new(t,id)
    }
}


// === Serialization & Deserialization === //

/// Literals used in `Ast` serialization and deserialization.
pub mod ast_schema {
    pub const STRUCT_NAME: &str      = "Ast";
    pub const SHAPE:       &str      = "shape";
    pub const ID:          &str      = "id";
    pub const LENGTH:      &str      = "span"; // scala parser is still using `span`
    pub const FIELDS:      [&str; 3] = [SHAPE, ID, LENGTH];
    pub const COUNT:       usize     = FIELDS.len();
}

impl Serialize for Ast {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        use ast_schema::*;
        let mut state = serializer.serialize_struct(STRUCT_NAME, COUNT)?;
        state.serialize_field(SHAPE, &self.shape())?;
        if self.id.is_some() {
            state.serialize_field(ID, &self.id)?;
        }
        state.serialize_field(LENGTH, &self.len)?;
        state.end()
    }
}

/// Type to provide serde::de::Visitor to deserialize data into `Ast`.
struct AstDeserializationVisitor;

impl<'de> Visitor<'de> for AstDeserializationVisitor {
    type Value = Ast;

    fn expecting
    (&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        use ast_schema::*;
        write!(formatter, "an object with `{}` and `{}` fields", SHAPE, LENGTH)
    }

    fn visit_map<A>
    (self, mut map: A) -> Result<Self::Value, A::Error>
    where A: serde::de::MapAccess<'de>, {
        use ast_schema::*;

        let mut shape: Option<Shape<Ast>> = None;
        let mut id:    Option<Option<ID>> = None;
        let mut len:   Option<usize>      = None;

        while let Some(key) = map.next_key()? {
            match key {
                SHAPE  => shape = Some(map.next_value()?),
                ID     => id    = Some(map.next_value()?),
                LENGTH => len   = Some(map.next_value()?),
                _      => {},
            }
        }

        let shape = shape.ok_or_else(|| serde::de::Error::missing_field(SHAPE))?;
        let id    = id.unwrap_or(None); // allow missing `id` field
        let len   = len.ok_or_else(|| serde::de::Error::missing_field(LENGTH))?;
        Ok(Ast::new_with_length(shape,id,len))
    }
}

impl<'de> Deserialize<'de> for Ast {
    fn deserialize<D>(deserializer: D) -> Result<Ast, D::Error>
    where D: Deserializer<'de> {
        use ast_schema::FIELDS;
        let visitor = AstDeserializationVisitor;
        deserializer.deserialize_struct("AstOf", &FIELDS, visitor)
    }
}



// =============
// === Shape ===
// =============

/// Defines shape of the subtree. Parametrized by the child node type `T`.
///
/// Shape describes names of children and spacing between them.
#[ast(flat)]
#[derive(HasTokens)]
pub enum Shape<T> {
    Unrecognized  { str : String   },
    InvalidQuote  { quote: Builder },
    InlineBlock   { quote: Builder },

    // === Identifiers ===
    Blank         { },
    Var           { name : String            },
    Cons          { name : String            },
    Opr           { name : String            },
    Mod           { name : String            },
    InvalidSuffix { elem : T, suffix: String },

    // === Number ===
    Number        { base: Option<String>, int: String },
    DanglingBase  { base: String                      },

    // === Text ===
    TextLineRaw   { text   : Vec<SegmentRaw>                  },
    TextLineFmt   { text   : Vec<SegmentFmt<T>>               },
    TextBlockRaw  { text   : Vec<TextBlockLine<SegmentRaw>>
                  , spaces : usize
                  , offset : usize                            },
    TextBlockFmt  { text   : Vec<TextBlockLine<SegmentFmt<T>>>
                  , spaces : usize
                  , offset : usize                            },
    TextUnclosed  { line   : TextLine<T>                      },

    // === Applications ===
    Prefix        { func : T,  off : usize, arg : T                         },
    Infix         { larg : T, loff : usize, opr : T, roff : usize, rarg : T },
    SectionLeft   {  arg : T,  off : usize, opr : T                         },
    SectionRight  {                         opr : T,  off : usize,  arg : T },
    SectionSides  {                         opr : T                         },

    // === Module ===
    Module        { lines       : Vec<BlockLine<Option<T>>>  },
    Block         { ty          : BlockType
                  , indent      : usize
                  , empty_lines : Vec<usize>
                  , first_line  : BlockLine<T>
                  , lines       : Vec<BlockLine<Option<T>>>
                  , is_orphan   : bool                       },

    // === Macros ===
    Match         { pfx      : Option<MacroPatternMatch<Shifted<Ast>>>
                  , segs     : ShiftedVec1<MacroMatchSegment<T>>
                  , resolved : Ast                                     },
    Ambiguous     { segs     : ShiftedVec1<MacroAmbiguousSegment>
                  , paths    : Tree<Ast, Unit>                         },

    // === Spaceless AST ===
    Comment       (Comment),
    Import        (Import<T>),
    Mixfix        (Mixfix<T>),
    Group         (Group<T>),
    Def           (Def<T>),
    Foreign       (Foreign),
}

/// Macrot that calls its argument (possibly other macro
#[macro_export]
macro_rules! with_shape_variants {
    ($f:ident) => {
        $f! { [Unrecognized] [InvalidQuote] [InlineBlock]
              [Blank] [Var] [Cons] [Opr] [Mod] [InvalidSuffix Ast]
              [Number] [DanglingBase]
              [TextLineRaw] [TextLineFmt Ast] [TextBlockRaw] [TextBlockFmt Ast] [TextUnclosed Ast]
              [Prefix Ast] [Infix Ast] [SectionLeft Ast] [SectionRight Ast] [SectionSides Ast]
              [Module Ast] [Block Ast]
              [Match Ast] [Ambiguous]
              // Note: Spaceless AST is intentionally omitted here.
            }
    };
}

// ===============
// === Builder ===
// ===============

#[ast(flat)]
#[derive(HasTokens)]
pub enum Builder {
    Empty,
    Letter{char: char},
    Space {span: usize},
    Text  {str : String},
    Seq   {first: Rc<Builder>, second: Rc<Builder>},
}



// ============
// === Text ===
// ============

// === Text Block Lines ===

#[ast] pub struct TextBlockLine<T> {
    pub empty_lines: Vec<usize>,
    pub text       : Vec<T>
}

#[ast(flat)]
#[derive(HasTokens)]
pub enum TextLine<T> {
    TextLineRaw(TextLineRaw),
    TextLineFmt(TextLineFmt<T>),
}


// === Text Segments ===
#[ast(flat)]
#[derive(HasTokens)]
pub enum SegmentRaw {
    SegmentPlain    (SegmentPlain),
    SegmentRawEscape(SegmentRawEscape),
}

#[ast(flat)]
#[derive(HasTokens)]
pub enum SegmentFmt<T> {
    SegmentPlain    (SegmentPlain    ),
    SegmentRawEscape(SegmentRawEscape),
    SegmentExpr     (SegmentExpr<T>  ),
    SegmentEscape   (SegmentEscape   ),
}

#[ast_node] pub struct SegmentPlain     { pub value: String    }
#[ast_node] pub struct SegmentRawEscape { pub code : RawEscape }
#[ast_node] pub struct SegmentExpr<T>   { pub value: Option<T> }
#[ast_node] pub struct SegmentEscape    { pub code : Escape    }


// === Text Segment Escapes ===

#[ast(flat)]
#[derive(HasTokens)]
pub enum RawEscape {
    Unfinished { },
    Invalid    { str: char },
    Slash      { },
    Quote      { },
    RawQuote   { },
}

#[ast]
#[derive(HasTokens)]
pub enum Escape {
    Character{c     :char            },
    Control  {name  :String, code: u8},
    Number   {digits:String          },
    Unicode16{digits:String          },
    Unicode21{digits:String          },
    Unicode32{digits:String          },
}



// =============
// === Block ===
// =============

#[ast_node] pub enum   BlockType     { Continuous { } , Discontinuous { } }
#[ast]      pub struct BlockLine <T> { pub elem: T, pub off: usize }



// =============
// === Macro ===
// =============

#[ast] pub struct MacroMatchSegment<T> {
    pub head : Ast,
    pub body : MacroPatternMatch<Shifted<T>>
}

#[ast] pub struct MacroAmbiguousSegment {
    pub head: Ast,
    pub body: Option<Shifted<Ast>>
}

pub type MacroPattern = Rc<MacroPatternRaw>;
#[ast] pub enum MacroPatternRaw {
    // === Boundary Patterns ===
    Begin   { },
    End     { },

    // === Structural Patterns ===
    Nothing { },
    Seq     { pat1 : MacroPattern , pat2    : MacroPattern                    },
    Or      { pat1 : MacroPattern , pat2    : MacroPattern                    },
    Many    { pat  : MacroPattern                                             },
    Except  { not  : MacroPattern, pat      : MacroPattern                    },

    // === Meta Patterns ===
    Build   { pat  : MacroPattern                                             },
    Err     { msg  : String       , pat     : MacroPattern                    },
    Tag     { tag  : String       , pat     : MacroPattern                    },
    Cls     { cls  : PatternClass , pat     : MacroPattern                    },

    // === Token Patterns ===
    Tok     { spaced : Spaced     , ast     : Ast                             },
    Blank   { spaced : Spaced                                                 },
    Var     { spaced : Spaced                                                 },
    Cons    { spaced : Spaced                                                 },
    Opr     { spaced : Spaced     , max_prec : Option<usize>                  },
    Mod     { spaced : Spaced                                                 },
    Num     { spaced : Spaced                                                 },
    Text    { spaced : Spaced                                                 },
    Block   { spaced : Spaced                                                 },
    Macro   { spaced : Spaced                                                 },
    Invalid { spaced : Spaced                                                 },
}

#[ast] pub enum PatternClass { Normal, Pattern }
pub type Spaced = Option<bool>;

// Note: Switch Implementation
#[ast(flat)]
pub enum Switch<T> { Left{value: T}, Right{value: T} }

// Note: Switch Implementation
// ~~~~~~~~~~~~~~~~~~~~~~~~~~~
// Switch is not defined as Either<T,T> because an iterator generated for such
// type would only iterate over right element, while we require both.
//
// Switch however does not need to be #[ast], when derive(Iterator) supports
// enum with struct variants, this attribute should be possible to remove.

impl<T> Switch<T> {
    fn get(&self) -> &T {
        match self {
            Switch::Left (elem) => &elem.value,
            Switch::Right(elem) => &elem.value,
        }
    }
}

pub type MacroPatternMatch<T> = Rc<MacroPatternMatchRaw<T>>;

#[ast]
#[derive(HasTokens)]
pub enum MacroPatternMatchRaw<T> {
    // === Boundary Matches ===
    Begin   { pat: MacroPatternRawBegin },
    End     { pat: MacroPatternRawEnd   },

    // === Structural Matches ===
    Nothing { pat: MacroPatternRawNothing                                     },
    Seq     { pat: MacroPatternRawSeq     , elem: (MacroPatternMatch<T>,
                                                   MacroPatternMatch<T>)      },
    Or      { pat: MacroPatternRawOr      , elem: Switch<MacroPatternMatch<T>>},
    Many    { pat: MacroPatternRawMany    , elem: Vec<MacroPatternMatch<T>>   },
    Except  { pat: MacroPatternRawExcept  , elem: MacroPatternMatch<T>        },

    // === Meta Matches ===
    Build   { pat: MacroPatternRawBuild   , elem: T                           },
    Err     { pat: MacroPatternRawErr     , elem: T                           },
    Tag     { pat: MacroPatternRawTag     , elem: MacroPatternMatch<T>        },
    Cls     { pat: MacroPatternRawCls     , elem: MacroPatternMatch<T>        },

    // === Token Matches ===
    Tok     { pat: MacroPatternRawTok     , elem: T                           },
    Blank   { pat: MacroPatternRawBlank   , elem: T                           },
    Var     { pat: MacroPatternRawVar     , elem: T                           },
    Cons    { pat: MacroPatternRawCons    , elem: T                           },
    Opr     { pat: MacroPatternRawOpr     , elem: T                           },
    Mod     { pat: MacroPatternRawMod     , elem: T                           },
    Num     { pat: MacroPatternRawNum     , elem: T                           },
    Text    { pat: MacroPatternRawText    , elem: T                           },
    Block   { pat: MacroPatternRawBlock   , elem: T                           },
    Macro   { pat: MacroPatternRawMacro   , elem: T                           },
    Invalid { pat: MacroPatternRawInvalid , elem: T                           },
}

// =============================================================================
// === Spaceless AST ===========================================================
// =============================================================================

#[ast] pub struct Comment {
    pub lines: Vec<String>
}

#[ast] pub struct Import<T> {
    pub path: Vec<T> // Cons inside
}

#[ast] pub struct Mixfix<T> {
    pub name: Vec<T>,
    pub args: Vec<T>,
}

#[ast] pub struct Group<T> {
    pub body: Option<T>,
}

#[ast] pub struct Def<T> {
    pub name: T, // being with Cons
    pub args: Vec<T>,
    pub body: Option<T>
}

#[ast] pub struct Foreign {
    pub indent : usize,
    pub lang   : String,
    pub code   : Vec<String>
}



// ===========
// === AST ===
// ===========


// === Tokenizer ===

/// An enum of valid Ast tokens.
#[derive(Debug)]
pub enum Token<'a> { Off(usize), Chr(char), Str(&'a str), Ast(&'a Ast) }

/// Things that can be turned into stream of tokens.
pub trait HasTokens {
    /// Feeds TokenBuilder with stream of tokens obtained from `self`.
    fn feed_to(&self, consumer:&mut impl TokenConsumer);
}

/// Helper trait for Tokenizer, which consumes the token stream.
pub trait TokenConsumer {
    /// consumes one token
    fn feed(&mut self, val:Token);
}


impl HasTokens for &str {
    fn feed_to(&self, consumer:&mut impl TokenConsumer) {
        consumer.feed(Token::Str(self));
    }
}

impl HasTokens for String {
    fn feed_to(&self, consumer:&mut impl TokenConsumer) {
        consumer.feed(Token::Str(self.as_str()));
    }
}

impl HasTokens for usize {
    fn feed_to(&self, consumer:&mut impl TokenConsumer) {
        consumer.feed(Token::Off(*self));
    }
}

impl HasTokens for char {
    fn feed_to(&self, consumer:&mut impl TokenConsumer) {
        consumer.feed(Token::Chr(*self));
    }
}

impl HasTokens for Ast {
    fn feed_to(&self, consumer:&mut impl TokenConsumer) {
        consumer.feed(Token::Ast(self));
    }
}

impl<T:HasTokens> HasTokens for Option<T> {
    fn feed_to(&self, consumer:&mut impl TokenConsumer) {
        for t in self { t.feed_to(consumer); }
    }
}

impl<T:HasTokens> HasTokens for Vec<T> {
    fn feed_to(&self, consumer:&mut impl TokenConsumer) {
        for t in self { t.feed_to(consumer); }
    }
}

impl<T:HasTokens> HasTokens for Rc<T> {
    fn feed_to(&self, consumer:&mut impl TokenConsumer) {
        self.content().feed_to(consumer);
    }
}

impl<T:HasTokens> HasTokens for &T {
    fn feed_to(&self, consumer:&mut impl TokenConsumer) {
        self.deref().feed_to(consumer);
    }
}

impl<T:HasTokens,U:HasTokens> HasTokens for (T,U) {
    fn feed_to(&self, consumer:&mut impl TokenConsumer) {
        self.0.feed_to(consumer);
        self.1.feed_to(consumer);
    }
}
impl<T:HasTokens,U:HasTokens,V:HasTokens> HasTokens for (T,U,V) {
    fn feed_to(&self, consumer:&mut impl TokenConsumer) {
        self.0.feed_to(consumer);
        self.1.feed_to(consumer);
        self.2.feed_to(consumer);
    }
}


// === HasIdMap ===

/// Things that have IdMap.
pub trait HasIdMap {
    /// Extracts IdMap from `self`.
    fn id_map(&self) -> IdMap;
}

#[derive(Debug,Clone,Default)]
struct IdMapBuilder { id_map:IdMap, offset:usize }

impl TokenConsumer for IdMapBuilder {
    fn feed(&mut self, token:Token) {
        match token {
            Token::Off(val) => self.offset += val,
            Token::Chr( _ ) => self.offset += 1,
            Token::Str(val) => self.offset += val.len(),
            Token::Ast(val) => {
                let begin = self.offset;
                val.shape().feed_to(self);
                if let Some(id) = val.id {
                    let span = Span::from((begin, self.offset));
                    self.id_map.insert(span, id);
                }
            }
        }
    }
}

impl<T:HasTokens> HasIdMap for T {
    fn id_map(&self) -> IdMap {
        let mut consumer = IdMapBuilder::default();
        self.feed_to(&mut consumer);
        consumer.id_map
    }
}


// === HasLength ===

/// Things that can be asked about their length.
pub trait HasLength {
    /// Length of the textual representation of This type in Unicode codepoints.
    ///
    /// Usually implemented together with `HasRepr`.For any `T:HasLength+HasRepr`
    /// for `t:T` the following must hold: `t.len() == t.repr().len()`.
    fn len(&self) -> usize;

    /// More efficient implementation of `t.len() == 0`
    fn is_empty(&self) -> bool { self.len() == 0 }
}

#[derive(Debug,Clone,Copy,Default)]
struct LengthBuilder { length:usize }

impl TokenConsumer for LengthBuilder {
    fn feed(&mut self, token:Token) {
        match token {
            Token::Off(val) => self.length += val,
            Token::Chr( _ ) => self.length += 1,
            Token::Str(val) => self.length += val.len(),
            Token::Ast(val) => val.shape().feed_to(self),
        }
    }
}

impl<T:HasTokens> HasLength for T {
    fn len(&self) -> usize {
        let mut consumer = LengthBuilder::default();
        self.feed_to(&mut consumer);
        consumer.length
    }
}


// === HasRepr ===

/// Things that can be asked about their textual representation.
///
/// See also `HasLength`.
pub trait HasRepr {
    /// Obtain the text representation for the This type.
    fn repr(&self) -> String;
}

#[derive(Debug,Clone,Default)]
struct ReprBuilder { repr:String }

impl TokenConsumer for ReprBuilder {
    fn feed(&mut self, token:Token) {
        match token {
            Token::Off(val) => self.repr.push_str(&" ".repeat(val)),
            Token::Chr(val) => self.repr.push(val),
            Token::Str(val) => self.repr.push_str(val),
            Token::Ast(val) => val.shape().feed_to(self),
        }
    }
}

impl<T:HasTokens> HasRepr for T {
    fn repr(&self) -> String {
        let mut consumer = ReprBuilder::default();
        self.feed_to(&mut consumer);
        consumer.repr
    }
}


// === WithID ===

pub type ID = Uuid;

pub trait HasID {
    fn id(&self) -> Option<ID>;
}

#[derive(Eq, PartialEq, Debug, Shrinkwrap, Serialize, Deserialize)]
#[shrinkwrap(mutable)]
pub struct WithID<T> {
    #[shrinkwrap(main_field)]
    #[serde(flatten)]
    pub wrapped: T,
    pub id: Option<ID>
}

impl<T> HasID for WithID<T>
    where T: HasID {
    fn id(&self) -> Option<ID> {
        self.id
    }
}

impl<T, S:Layer<T>>
Layer<T> for WithID<S> {
    fn layered(t: T) -> Self {
        WithID { wrapped: Layer::layered(t), id: None }
    }
}

impl<T> HasLength for WithID<T>
where T:HasLength {
    fn len(&self) -> usize {
        self.deref().len()
    }
}


// === WithLength ===

/// Stores a value of type `T` and information about its length.
///
/// Even if `T` is `Spanned`, keeping `length` variable is desired for performance
/// purposes.
#[derive(Eq, PartialEq, Debug, Shrinkwrap, Serialize, Deserialize)]
#[shrinkwrap(mutable)]
pub struct WithLength<T> {
    #[shrinkwrap(main_field)]
    #[serde(flatten)]
    pub wrapped: T,
    pub len: usize
}

impl<T> HasLength for WithLength<T> {
    fn len(&self) -> usize { self.len }
}

impl<T, S> Layer<T> for WithLength<S>
where T: HasLength + Into<S> {
    fn layered(t: T) -> Self {
        let length = t.len();
        WithLength { wrapped: t.into(), len: length }
    }
}

impl<T> HasID for WithLength<T>
    where T: HasID {
    fn id(&self) -> Option<ID> {
        self.deref().id()
    }
}


// =============================================================================
// === TO BE GENERATED =========================================================
// =============================================================================
// TODO: the definitions below should be removed and instead generated using
//  macros, as part of https://github.com/luna/enso/issues/338


// === AST ===

impl Ast {
    // TODO smart constructors for other cases
    //  as part of https://github.com/luna/enso/issues/338

    pub fn number(number:i64) -> Ast {
        let number = Number {base:None,int:number.to_string()};
        Ast::from(number)
    }

    pub fn cons<Str: ToString>(name:Str) -> Ast {
        let cons = Cons {name:name.to_string()};
        Ast::from(cons)
    }

    pub fn var<Str: ToString>(name:Str) -> Ast {
        let var = Var{name:name.to_string()};
        Ast::from(var)
    }

    pub fn opr<Str: ToString>(name:Str) -> Ast {
        let opr = Opr{name:name.to_string() };
        Ast::from(opr)
    }

    pub fn prefix<Func:Into<Ast>, Arg:Into<Ast>>(func:Func, arg:Arg) -> Ast {
        let off = 1;
        let opr = Prefix{ func:func.into(), off, arg:arg.into() };
        Ast::from(opr)
    }

    /// Creates an AST node with `Infix` shape, where both its operands are Vars.
    pub fn infix_var<Str0, Str1, Str2>(larg:Str0, opr:Str1, rarg:Str2) -> Ast
    where Str0: ToString
        , Str1: ToString
        , Str2: ToString {
        let larg  = Ast::var(larg);
        let loff  = 1;
        let opr   = Ast::opr(opr);
        let roff  = 1;
        let rarg  = Ast::var(rarg);
        let infix = Infix { larg, loff, opr, roff, rarg };
        Ast::from(infix)
    }
}


// === Text Conversion Boilerplate ===

// support for transitive conversions, like:
// RawEscapeSth -> RawEscape -> SegmentRawEscape -> SegmentRaw

impl From<Unfinished> for SegmentRaw {
    fn from(value: Unfinished) -> Self {
        SegmentRawEscape{ code: value.into() }.into()
    }
}
impl From<Invalid> for SegmentRaw {
    fn from(value: Invalid) -> Self {
        SegmentRawEscape{ code: value.into() }.into()
    }
}
impl From<Slash> for SegmentRaw {
    fn from(value: Slash) -> Self {
        SegmentRawEscape{ code: value.into() }.into()
    }
}
impl From<Quote> for SegmentRaw {
    fn from(value: Quote) -> Self {
        SegmentRawEscape{ code: value.into() }.into()
    }
}
impl From<RawQuote> for SegmentRaw {
    fn from(value: RawQuote) -> Self {
        SegmentRawEscape{ code: value.into() }.into()
    }
}


// === RawEscapeSth -> RawEscape -> SegmentRawEscape -> SegmentFmt ===

impl<T> From<Unfinished> for SegmentFmt<T> {
    fn from(value: Unfinished) -> Self {
        SegmentRawEscape{ code: value.into() }.into()
    }
}
impl<T> From<Invalid> for SegmentFmt<T> {
    fn from(value: Invalid) -> Self {
        SegmentRawEscape{ code: value.into() }.into()
    }
}
impl<T> From<Slash> for SegmentFmt<T> {
    fn from(value: Slash) -> Self {
        SegmentRawEscape{ code: value.into() }.into()
    }
}
impl<T> From<Quote> for SegmentFmt<T> {
    fn from(value: Quote) -> Self {
        SegmentRawEscape{ code: value.into() }.into()
    }
}
impl<T> From<RawQuote> for SegmentFmt<T> {
    fn from(value: RawQuote) -> Self {
        SegmentRawEscape{ code: value.into() }.into()
    }
}

impl<T> From<Escape> for SegmentFmt<T> {
    fn from(value: Escape) -> Self {
        SegmentEscape{ code: value }.into()
    }
}


// === EscapeSth -> Escape -> SegmentEscape -> SegmentFmt ===

impl<T> From<EscapeCharacter> for SegmentFmt<T> {
    fn from(value: EscapeCharacter) -> Self {
        SegmentEscape{ code: value.into() }.into()
    }
}

impl<T> From<EscapeControl> for SegmentFmt<T> {
    fn from(value: EscapeControl) -> Self {
        SegmentEscape{ code: value.into() }.into()
    }
}

impl<T> From<EscapeNumber> for SegmentFmt<T> {
    fn from(value: EscapeNumber) -> Self {
        SegmentEscape{ code: value.into() }.into()
    }
}

impl<T> From<EscapeUnicode16> for SegmentFmt<T> {
    fn from(value: EscapeUnicode16) -> Self {
        SegmentEscape{ code: value.into() }.into()
    }
}

impl<T> From<EscapeUnicode21> for SegmentFmt<T> {
    fn from(value: EscapeUnicode21) -> Self {
        SegmentEscape{ code: value.into() }.into()
    }
}

impl<T> From<EscapeUnicode32> for SegmentFmt<T> {
    fn from(value: EscapeUnicode32) -> Self {
        SegmentEscape{ code: value.into() }.into()
    }
}



// =============
// === Tests ===
// =============

#[cfg(test)]
mod tests {
    use super::*;
    use serde::de::DeserializeOwned;

    /// Assert that given value round trips JSON serialization.
    fn round_trips<T>(input_val: &T)
    where T: Serialize + DeserializeOwned + PartialEq + Debug {
        let json_str            = serde_json::to_string(&input_val).unwrap();
        let deserialized_val: T = serde_json::from_str(&json_str).unwrap();
        assert_eq!(*input_val, deserialized_val);
    }

    #[test]
    fn var_smart_constructor() {
        let name = "foo".to_string();
        let v    = Ast::var(name.clone());
        match v.shape() {
            Shape::Var(var) if *var.name == name =>
                (),
            _ =>
                panic!("expected Var with name `{}`", name),
        }
    }

    #[test]
    fn ast_length() {
        let ast = Ast::prefix(Ast::var("XX"), Ast::var("YY"));
        assert_eq!(ast.len(), 5)
    }

    #[test]
    fn ast_repr() {
        let ast = Ast::prefix(Ast::var("XX"), Ast::var("YY"));
        assert_eq!(ast.repr().as_str(), "XX YY")
    }

    #[test]
    fn ast_id_map() {
        let span = |ix,length| Span::from((ix,length));
        let uid  = default();
        let ids  = vec![(span(0,2),uid), (span(3,5),uid), (span(0,5),uid)];
        let func = Ast::new(Var    {name:"XX".into()}, Some(uid));
        let arg  = Ast::new(Var    {name:"YY".into()}, Some(uid));
        let ast  = Ast::new(Prefix {func,off:1,arg  }, Some(uid));
        assert_eq!(ast.id_map(), IdMap(ids));
    }

    #[test]
    fn ast_wrapping() {
        // We can convert `Var` into AST without worrying about length nor id.
        let ident = "foo".to_string();
        let v     = Var{ name: ident.clone() };
        let ast   = Ast::from(v);
        assert_eq!(ast.wrapped.id, None);
        assert_eq!(ast.wrapped.wrapped.len, ident.len());
    }

    #[test]
    fn serialization_round_trip() {
        let make_var = || Var { name: "foo".into() };
        round_trips(&make_var());

        let ast_without_id = Ast::new(make_var(), None);
        round_trips(&ast_without_id);

        let id        = Uuid::parse_str("15").ok();
        let ast_with_id = Ast::new(make_var(), id);
        round_trips(&ast_with_id);
    }

    #[test]
    fn deserialize_var() {
        let var_name = "foo";
        let uuid_str = "51e74fb9-75a4-499d-9ea3-a90a2663b4a1";

        let sample_json = serde_json::json!({
            "shape": { "Var":{"name": var_name}},
            "id": uuid_str,
            "span": var_name.len()
        });
        let sample_json_text = sample_json.to_string();
        let ast: Ast         = serde_json::from_str(&sample_json_text).unwrap();

        let expected_uuid = Uuid::parse_str(uuid_str).ok();
        assert_eq!(ast.id, expected_uuid);

        let expected_length = 3;
        assert_eq!(ast.len, expected_length);

        let expected_var   = Var { name: var_name.into() };
        let expected_shape = Shape::from(expected_var);
        assert_eq!(*ast.shape(), expected_shape);
    }

    #[test]
    /// Check if Ast can be iterated.
    fn iterating() {
        // TODO [mwu] When Repr is implemented, the below lambda sohuld be
        //            removed in favor of it.
        let to_string = |ast:&Ast| match ast.shape() {
            Shape::Var(var)   => var.name   .clone(),
            Shape::Opr(opr)   => opr.name   .clone(),
            _                 => "«invalid»".to_string(),
        };

        let infix   = Ast::infix_var("foo", "+", "bar");
        let strings = infix.iter().map(to_string);
        let strings = strings.collect::<Vec<_>>();

        let assert_contains = |searched:&str| {
           assert!(strings.iter().any(|elem| elem == searched))
        };
        assert_contains("foo");
        assert_contains("bar");
        assert_contains("+");
        assert_eq!(strings.len(), 3);
    }

    #[test]
    fn iterate_nested() {
        let a   = Ast::var("a");
        let b   = Ast::var("b");
        let c   = Ast::var("c");
        let ab  = Ast::prefix(a,b);
        let abc = Ast::prefix(ab, c); // repr is `a b c`

        assert_eq!((&abc).iter().count(), 2); // for App's two children
        assert_eq!(abc.iter_recursive().count(), 5); // for 2 Apps and 3 Vars
    }
}
