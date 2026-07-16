(package_clause (package_identifier) @definition.module)

(import_spec
  name: (package_identifier) @definition.import)
(import_spec
  path: (interpreted_string_literal
    (interpreted_string_literal_content) @definition.import))
(import_spec
  path: (raw_string_literal) @definition.import)

(function_declaration name: (identifier) @definition.function)
(method_declaration name: (field_identifier) @definition.method)
(method_elem name: (field_identifier) @definition.method)

(type_spec name: (type_identifier) @definition.type)
(type_alias name: (type_identifier) @definition.type)

(field_declaration name: (field_identifier) @definition.field)
(const_spec name: (identifier) @definition.const)
(var_spec name: (identifier) @definition.variable)
(short_var_declaration
  left: (expression_list
    (identifier) @definition.variable))
