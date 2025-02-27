/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::num::NonZeroU32;

use hermes::parser::hermes_get_ArrowFunctionExpression_async;
use hermes::parser::hermes_get_ArrowFunctionExpression_body;
use hermes::parser::hermes_get_ArrowFunctionExpression_expression;
use hermes::parser::hermes_get_ArrowFunctionExpression_id;
use hermes::parser::hermes_get_ArrowFunctionExpression_params;
use hermes::parser::hermes_get_ClassDeclaration_body;
use hermes::parser::hermes_get_ClassDeclaration_id;
use hermes::parser::hermes_get_ClassDeclaration_superClass;
use hermes::parser::hermes_get_ClassExpression_body;
use hermes::parser::hermes_get_ClassExpression_id;
use hermes::parser::hermes_get_ClassExpression_superClass;
use hermes::parser::hermes_get_FunctionDeclaration_async;
use hermes::parser::hermes_get_FunctionDeclaration_body;
use hermes::parser::hermes_get_FunctionDeclaration_generator;
use hermes::parser::hermes_get_FunctionDeclaration_id;
use hermes::parser::hermes_get_FunctionDeclaration_params;
use hermes::parser::hermes_get_FunctionExpression_async;
use hermes::parser::hermes_get_FunctionExpression_body;
use hermes::parser::hermes_get_FunctionExpression_generator;
use hermes::parser::hermes_get_FunctionExpression_id;
use hermes::parser::hermes_get_FunctionExpression_params;
use hermes::parser::hermes_get_Property_computed;
use hermes::parser::hermes_get_Property_key;
use hermes::parser::hermes_get_Property_kind;
use hermes::parser::hermes_get_Property_method;
use hermes::parser::hermes_get_Property_shorthand;
use hermes::parser::hermes_get_Property_value;
use hermes::parser::Comment as HermesComment;
use hermes::parser::NodeKind;
use hermes::parser::NodeLabel;
use hermes::parser::NodeLabelOpt;
use hermes::parser::NodeListRef;
use hermes::parser::NodePtr;
use hermes::parser::NodePtrOpt;
use hermes::parser::NodeString;
use hermes::parser::NodeStringOpt;
use hermes::parser::SMRange;
use hermes::utf::utf8_with_surrogates_to_string;
use hermes_estree::ArrowFunctionExpression;
use hermes_estree::AssignmentProperty;
use hermes_estree::Class;
use hermes_estree::ClassBody;
use hermes_estree::ClassDeclaration;
use hermes_estree::ClassExpression;
use hermes_estree::Expression;
use hermes_estree::ExpressionOrSpread;
use hermes_estree::Function;
use hermes_estree::FunctionBody;
use hermes_estree::FunctionDeclaration;
use hermes_estree::FunctionExpression;
use hermes_estree::Identifier;
use hermes_estree::Number;
use hermes_estree::Pattern;
use hermes_estree::SourceRange;
use hermes_estree::TemplateElement;
use hermes_estree::TemplateElementValue;
use juno_support::NullTerminatedBuf;
use serde::Serialize;

pub struct Context {
    start: usize,
}

impl Context {
    pub fn new(parser: &NullTerminatedBuf) -> Self {
        // SAFETY: This function returns a pointer to the underlying
        // buffer. It is safe to get the pointer, it is only unsafe to
        // use that pointer in unsafe ways. We only use the value to
        // calculate offsets (and only use safe APIs to access the string
        // based on those offsets).
        let ptr = unsafe { parser.as_ptr() };
        let start = ptr as usize;
        Self { start }
    }
}

pub trait FromHermes {
    fn convert(cx: &mut Context, node: NodePtr) -> Self;
}
pub trait FromHermesLabel {
    fn convert(cx: &mut Context, label: NodeLabel) -> Self;
}

pub fn convert_option<F, T>(node: NodePtrOpt, f: F) -> Option<T>
where
    F: FnMut(NodePtr) -> T,
{
    node.as_node_ptr().map(f)
}

pub fn convert_vec<F, T>(node: NodeListRef, mut f: F) -> Vec<T>
where
    F: FnMut(NodePtr) -> T,
{
    node.iter().map(|node| f(NodePtr::new(node))).collect()
}

pub fn convert_vec_of_option<F, T>(node: NodeListRef, mut f: F) -> Vec<Option<T>>
where
    F: FnMut(NodePtr) -> T,
{
    node.iter()
        .map(|node| {
            let node = NodePtr::new(node);
            let node_ref = node.as_ref();
            match node_ref.kind {
                NodeKind::Empty => None,
                _ => Some(f(node)),
            }
        })
        .collect()
}

pub fn convert_range(cx: &Context, node: NodePtr) -> SourceRange {
    let range = node.as_ref().source_range;
    convert_smrange(cx, range)
}

pub fn convert_smrange(cx: &Context, range: SMRange) -> SourceRange {
    let absolute_start: usize = range.start.as_ptr() as usize;
    let start = absolute_start - cx.start;
    let absolute_end: usize = range.end.as_ptr() as usize;
    let end = absolute_end - cx.start;
    SourceRange {
        start: start as u32,
        end: NonZeroU32::new(end as u32).unwrap(),
    }
}

pub fn convert_string(_cx: &mut Context, label: NodeLabel) -> String {
    utf8_with_surrogates_to_string(label.as_slice()).unwrap()
}

#[allow(dead_code)]
pub fn convert_option_string(cx: &mut Context, label: NodeLabelOpt) -> Option<String> {
    label.as_node_label().map(|label| convert_string(cx, label))
}

pub fn convert_string_value(_cx: &mut Context, label: NodeString) -> String {
    utf8_with_surrogates_to_string(label.as_slice()).unwrap()
}

pub fn convert_option_string_value(_cx: &mut Context, label: NodeStringOpt) -> Option<String> {
    label
        .as_node_string()
        .map(|label| utf8_with_surrogates_to_string(label.as_slice()).unwrap())
}

pub fn convert_number(value: f64) -> Number {
    value.into()
}

pub fn convert_array_expression_elements(
    cx: &mut Context,
    node: NodeListRef,
) -> Vec<Option<ExpressionOrSpread>> {
    convert_vec_of_option(node, |node| ExpressionOrSpread::convert(cx, node))
}

impl FromHermes for TemplateElement {
    fn convert(cx: &mut Context, node: NodePtr) -> Self {
        let range = convert_range(cx, node);
        let tail = unsafe { hermes::parser::hermes_get_TemplateElement_tail(node) };
        let value = TemplateElementValue {
            cooked: convert_option_string_value(cx, unsafe {
                hermes::parser::hermes_get_TemplateElement_cooked(node)
            }),
            raw: convert_string(cx, unsafe {
                hermes::parser::hermes_get_TemplateElement_raw(node)
            }),
        };
        Self {
            tail,
            value,
            loc: None,
            range,
        }
    }
}

impl FromHermes for AssignmentProperty {
    fn convert(cx: &mut Context, node: NodePtr) -> Self {
        let key = FromHermes::convert(cx, unsafe { hermes_get_Property_key(node) });
        let value = FromHermes::convert(cx, unsafe { hermes_get_Property_value(node) });
        let kind = FromHermesLabel::convert(cx, unsafe { hermes_get_Property_kind(node) });
        let is_method = unsafe { hermes_get_Property_method(node) };
        let is_computed = unsafe { hermes_get_Property_computed(node) };
        let is_shorthand = unsafe { hermes_get_Property_shorthand(node) };
        let loc = None;
        let range = convert_range(cx, node);
        AssignmentProperty {
            key,
            value,
            kind,
            is_method,
            is_computed,
            is_shorthand,
            loc,
            range,
        }
    }
}

impl FromHermes for FunctionDeclaration {
    fn convert(cx: &mut Context, node: NodePtr) -> Self {
        let id = convert_option(unsafe { hermes_get_FunctionDeclaration_id(node) }, |node| {
            Identifier::convert(cx, node)
        });
        let params = convert_vec(
            unsafe { hermes_get_FunctionDeclaration_params(node) },
            |node| Pattern::convert(cx, node),
        );
        let body = FunctionBody::convert(cx, unsafe { hermes_get_FunctionDeclaration_body(node) });
        let is_generator = unsafe { hermes_get_FunctionDeclaration_generator(node) };
        let is_async = unsafe { hermes_get_FunctionDeclaration_async(node) };
        let loc = None;
        let range = convert_range(cx, node);
        FunctionDeclaration {
            function: Function {
                id,
                params,
                body: Some(body),
                is_generator,
                is_async,
                loc: loc.clone(),
                range,
            },
            loc,
            range,
        }
    }
}

impl FromHermes for FunctionExpression {
    fn convert(cx: &mut Context, node: NodePtr) -> Self {
        let id = convert_option(unsafe { hermes_get_FunctionExpression_id(node) }, |node| {
            Identifier::convert(cx, node)
        });
        let params = convert_vec(
            unsafe { hermes_get_FunctionExpression_params(node) },
            |node| Pattern::convert(cx, node),
        );
        let body = FunctionBody::convert(cx, unsafe { hermes_get_FunctionExpression_body(node) });
        let is_generator = unsafe { hermes_get_FunctionExpression_generator(node) };
        let is_async = unsafe { hermes_get_FunctionExpression_async(node) };
        let loc = None;
        let range = convert_range(cx, node);
        FunctionExpression {
            function: Function {
                id,
                params,
                body: Some(body),
                is_generator,
                is_async,
                loc: loc.clone(),
                range,
            },
            loc,
            range,
        }
    }
}

impl FromHermes for ArrowFunctionExpression {
    fn convert(cx: &mut Context, node: NodePtr) -> Self {
        let id = convert_option(
            unsafe { hermes_get_ArrowFunctionExpression_id(node) },
            |node| Identifier::convert(cx, node),
        );
        let params = convert_vec(
            unsafe { hermes_get_ArrowFunctionExpression_params(node) },
            |node| Pattern::convert(cx, node),
        );
        let body =
            FunctionBody::convert(cx, unsafe { hermes_get_ArrowFunctionExpression_body(node) });
        let is_generator = unsafe { hermes_get_FunctionExpression_generator(node) };
        let is_async = unsafe { hermes_get_ArrowFunctionExpression_async(node) };
        let is_expression = unsafe { hermes_get_ArrowFunctionExpression_expression(node) };
        let loc = None;
        let range = convert_range(cx, node);
        ArrowFunctionExpression {
            function: Function {
                id,
                params,
                body: Some(body),
                is_generator,
                is_async,
                loc: loc.clone(),
                range,
            },
            is_expression,
            loc,
            range,
        }
    }
}

impl FromHermes for ClassDeclaration {
    fn convert(cx: &mut Context, node: NodePtr) -> Self {
        let id = convert_option(unsafe { hermes_get_ClassDeclaration_id(node) }, |node| {
            Identifier::convert(cx, node)
        });
        let super_class = convert_option(
            unsafe { hermes_get_ClassDeclaration_superClass(node) },
            |node| Expression::convert(cx, node),
        );
        let body = ClassBody::convert(cx, unsafe { hermes_get_ClassDeclaration_body(node) });
        let loc = None;
        let range = convert_range(cx, node);
        ClassDeclaration {
            class: Class {
                id,
                super_class,
                body,
                range,
            },
            loc,
            range,
        }
    }
}
impl FromHermes for ClassExpression {
    fn convert(cx: &mut Context, node: NodePtr) -> Self {
        let id = convert_option(unsafe { hermes_get_ClassExpression_id(node) }, |node| {
            Identifier::convert(cx, node)
        });
        let super_class = convert_option(
            unsafe { hermes_get_ClassExpression_superClass(node) },
            |node| Expression::convert(cx, node),
        );
        let body = ClassBody::convert(cx, unsafe { hermes_get_ClassExpression_body(node) });
        let loc = None;
        let range = convert_range(cx, node);
        ClassExpression {
            class: Class {
                id,
                super_class,
                body,
                range,
            },
            loc,
            range,
        }
    }
}

#[derive(Serialize, Debug)]
pub struct Comment {
    pub value: String,
    pub range: SourceRange,
}

pub fn convert_comment(cx: &mut Context, comment: &HermesComment) -> Comment {
    let str = comment.get_string();
    let range = convert_smrange(cx, comment.source_range);

    Comment {
        value: str.to_string(),
        range,
    }
}
