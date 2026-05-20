(function_declaration
  name: (identifier) @name) @symbol.fn

(class_declaration
  name: (type_identifier) @name) @symbol.class

(interface_declaration
  name: (type_identifier) @name) @symbol.interface

(type_alias_declaration
  name: (type_identifier) @name) @symbol.type

(enum_declaration
  name: (identifier) @name) @symbol.enum

(lexical_declaration
  "const"
  (variable_declarator
    name: (identifier) @name)) @symbol.const
