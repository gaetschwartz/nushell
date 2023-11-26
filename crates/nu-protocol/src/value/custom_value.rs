use std::{cmp::Ordering, fmt, path::PathBuf};

use chrono::{DateTime, FixedOffset};

use crate::{
    ast::{CellPath, MatchPattern, Operator},
    engine::Closure,
    BlockId, LazyRecord, Range, Record, ShellError, Span, Spanned, Value,
};

// Trait definition for a custom value
#[typetag::serde(tag = "type")]
pub trait CustomValue: fmt::Debug + Send + Sync {
    fn clone_value(&self, span: Span) -> Value;

    //fn category(&self) -> Category;

    // Define string representation of the custom value
    fn value_string(&self) -> String;

    // Converts the custom value to a base nushell value
    // This is used to represent the custom value using the table representations
    // That already exist in nushell
    fn to_base_value(&self, span: Span) -> Result<Value, ShellError>;

    // Any representation used to downcast object to its original type
    fn as_any(&self) -> &dyn std::any::Any;

    // Follow cell path functions
    fn follow_path_int(&self, _count: usize, span: Span) -> Result<Value, ShellError> {
        Err(ShellError::IncompatiblePathAccess {
            type_name: self.value_string(),
            span,
        })
    }

    fn follow_path_string(&self, _column_name: String, span: Span) -> Result<Value, ShellError> {
        Err(ShellError::IncompatiblePathAccess {
            type_name: self.value_string(),
            span,
        })
    }

    // ordering with other value
    fn partial_cmp(&self, _other: &Value) -> Option<Ordering> {
        None
    }

    fn span(&self) -> Span {
        Span::unknown()
    }

    // Definition of an operation between the object that implements the trait
    // and another Value.
    // The Operator enum is used to indicate the expected operation
    fn operation(
        &self,
        _lhs_span: Span,
        operator: Operator,
        op: Span,
        _right: &Value,
    ) -> Result<Value, ShellError> {
        Err(ShellError::UnsupportedOperator { operator, span: op })
    }

    fn as_bool(&self) -> Result<bool, ShellError> {
        Err(ShellError::CantConvert {
            to_type: "boolean".into(),
            from_type: self.typetag_name().into(),
            span: self.span(),
            help: None,
        })
    }

    fn as_int(&self) -> Result<i64, ShellError> {
        Err(ShellError::CantConvert {
            to_type: "int".into(),
            from_type: self.typetag_name().into(),
            span: self.span(),
            help: None,
        })
    }

    fn as_float(&self) -> Result<f64, ShellError> {
        Err(ShellError::CantConvert {
            to_type: "float".into(),
            from_type: self.typetag_name().into(),
            span: self.span(),
            help: None,
        })
    }

    fn as_filesize(&self) -> Result<i64, ShellError> {
        Err(ShellError::CantConvert {
            to_type: "filesize".into(),
            from_type: self.typetag_name().into(),
            span: self.span(),
            help: None,
        })
    }

    fn as_duration(&self) -> Result<i64, ShellError> {
        Err(ShellError::CantConvert {
            to_type: "duration".into(),
            from_type: self.typetag_name().into(),
            span: self.span(),
            help: None,
        })
    }

    fn as_date(&self) -> Result<DateTime<FixedOffset>, ShellError> {
        Err(ShellError::CantConvert {
            to_type: "date".into(),
            from_type: self.typetag_name().into(),
            span: self.span(),
            help: None,
        })
    }

    fn as_range(&self) -> Result<&Range, ShellError> {
        Err(ShellError::CantConvert {
            to_type: "range".into(),
            from_type: self.typetag_name().into(),
            span: self.span(),
            help: None,
        })
    }

    fn as_string(&self) -> Result<String, ShellError> {
        Err(ShellError::CantConvert {
            to_type: "string".into(),
            from_type: self.typetag_name().into(),
            span: self.span(),
            help: None,
        })
    }

    fn as_spanned_string(&self) -> Result<Spanned<String>, ShellError> {
        Err(ShellError::CantConvert {
            to_type: "string".into(),
            from_type: self.typetag_name().into(),
            span: self.span(),
            help: None,
        })
    }

    fn as_char(&self) -> Result<char, ShellError> {
        Err(ShellError::CantConvert {
            to_type: "char".into(),
            from_type: self.typetag_name().into(),
            span: self.span(),
            help: None,
        })
    }

    fn as_path(&self) -> Result<PathBuf, ShellError> {
        Err(ShellError::CantConvert {
            to_type: "path".into(),
            from_type: self.typetag_name().into(),
            span: self.span(),
            help: None,
        })
    }

    fn as_record(&self) -> Result<&Record, ShellError> {
        Err(ShellError::CantConvert {
            to_type: "record".into(),
            from_type: self.typetag_name().into(),
            span: self.span(),
            help: None,
        })
    }

    fn as_list(&self) -> Result<&[Value], ShellError> {
        Err(ShellError::CantConvert {
            to_type: "list".into(),
            from_type: self.typetag_name().into(),
            span: self.span(),
            help: None,
        })
    }

    fn as_block(&self) -> Result<BlockId, ShellError> {
        Err(ShellError::CantConvert {
            to_type: "block".into(),
            from_type: self.typetag_name().into(),
            span: self.span(),
            help: None,
        })
    }

    fn as_closure(&self) -> Result<&Closure, ShellError> {
        Err(ShellError::CantConvert {
            to_type: "closure".into(),
            from_type: self.typetag_name().into(),
            span: self.span(),
            help: None,
        })
    }

    fn as_binary(&self) -> Result<&[u8], ShellError> {
        Err(ShellError::CantConvert {
            to_type: "binary".into(),
            from_type: self.typetag_name().into(),
            span: self.span(),
            help: None,
        })
    }

    fn as_cell_path(&self) -> Result<&CellPath, ShellError> {
        Err(ShellError::CantConvert {
            to_type: "cell path".into(),
            from_type: self.typetag_name().into(),
            span: self.span(),
            help: None,
        })
    }

    fn as_custom_value(&self) -> Result<&dyn CustomValue, ShellError>
    where
        Self: Sized,
    {
        Ok(self)
    }

    fn as_lazy_record(&self) -> Result<&dyn for<'a> LazyRecord<'a>, ShellError> {
        Err(ShellError::CantConvert {
            to_type: "lazy record".into(),
            from_type: self.typetag_name().into(),
            span: self.span(),
            help: None,
        })
    }

    fn as_match_pattern(&self) -> Result<&MatchPattern, ShellError> {
        Err(ShellError::CantConvert {
            to_type: "match-pattern".into(),
            from_type: self.typetag_name().into(),
            span: self.span(),
            help: None,
        })
    }
}
