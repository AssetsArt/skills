(function_item
  name: (identifier) @name) @symbol.fn

(struct_item
  name: (type_identifier) @name) @symbol.struct

(enum_item
  name: (type_identifier) @name) @symbol.enum

(trait_item
  name: (type_identifier) @name) @symbol.trait

(type_item
  name: (type_identifier) @name) @symbol.type

(const_item
  name: (identifier) @name) @symbol.const

(static_item
  name: (identifier) @name) @symbol.const
