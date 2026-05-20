(module
  (function_definition
    name: (identifier) @name) @symbol.fn)

(module
  (class_definition
    name: (identifier) @name) @symbol.class)

(module
  (decorated_definition
    definition: (function_definition
      name: (identifier) @name) @symbol.fn))

(module
  (decorated_definition
    definition: (class_definition
      name: (identifier) @name) @symbol.class))
