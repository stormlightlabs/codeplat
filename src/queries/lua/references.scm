(function_call
  name: (identifier) @reference.function)

(function_call
  name: (dot_index_expression
    field: (identifier) @reference.function))

(function_call
  name: (method_index_expression
    method: (identifier) @reference.method))

(dot_index_expression
  field: (identifier) @reference.field)

(method_index_expression
  method: (identifier) @reference.field)

(identifier) @reference.identifier
