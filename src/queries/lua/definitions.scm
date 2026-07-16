(function_declaration
  name: (identifier) @definition.function)

(function_declaration
  name: (dot_index_expression
    field: (identifier) @definition.function))

(function_declaration
  name: (method_index_expression
    method: (identifier) @definition.method))

(variable_declaration
  (assignment_statement
    (variable_list
      (identifier) @definition.variable)))

(variable_declaration
  (variable_list
    (identifier) @definition.variable))

(assignment_statement
  (variable_list
    (identifier) @definition.variable))

(assignment_statement
  (variable_list
    (dot_index_expression
      field: (identifier) @definition.field)))

(assignment_statement
  (variable_list
    .
    name: [
      (identifier) @definition.function
      (dot_index_expression
        field: (identifier) @definition.function)
    ])
  (expression_list
    .
    value: (function_definition)))

(field
  name: (identifier) @definition.field)

(field
  name: (identifier) @definition.function
  value: (function_definition))

(function_call
  name: (identifier) @_require
  arguments: (arguments
    (string
      content: (string_content) @definition.import))
  (#eq? @_require "require"))
