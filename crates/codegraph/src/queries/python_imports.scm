; from .user import User, new_user as nu
(import_from_statement
  module_name: (_) @path
  name: (dotted_name (identifier) @name)) @import

(import_from_statement
  module_name: (_) @path
  name: (aliased_import
    name: (dotted_name (identifier) @name)
    alias: (identifier) @alias)) @import

; import foo
(import_statement
  name: (dotted_name (identifier) @name)) @import

; import foo as bar
(import_statement
  name: (aliased_import
    name: (dotted_name (identifier) @name)
    alias: (identifier) @alias)) @import
