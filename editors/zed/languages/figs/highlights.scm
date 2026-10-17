; Base highlighting from the Rhai grammar. With `"semantic_tokens": "combined"`
; figs-lsp refines it (core builders and prelude functions vs. the script's own).

(comment_line) @comment
(comment_block) @comment
(comment_line_doc) @comment.doc
(comment_block_doc) @comment.doc

(lit_str) @string
(lit_char) @string
(lit_int) @number
(lit_float) @number
(lit_bool) @boolean

(FnDeclName (ident) @function)
; `f(x)` and `a::f(x)`; a name parses as a path.
(ExprCall fn_name: (Expr (ExprPath (Path (ident) @function .))))
; `x.f(y)`: the call's callee is the dot access, its last operand the name.
(ExprCall
  fn_name: (Expr (ExprDotAccess
    (Expr (ExprPath (Path (ident) @function.method .))) .)))
(Param (ident) @variable.parameter)
(ExprDeclareVar name: (ident) @variable)
(ObjectField key: (ident) @property)

[
  "let" "const" "fn" "private" "if" "else" "switch" "while" "loop" "for" "in"
  "do" "until" "break" "return" "throw" "try" "catch" "import"
  "export" "as"
] @keyword
(ExprContinue) @keyword

["(" ")" "[" "]" "{" "}" "#{"] @punctuation.bracket
["," ";" "." "?." ":" "::"] @punctuation.delimiter
"=>" @operator
