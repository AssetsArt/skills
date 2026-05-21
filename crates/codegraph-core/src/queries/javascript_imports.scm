(import_statement
  (import_clause
    (named_imports
      (import_specifier
        name: (identifier) @name
        alias: (identifier)? @alias)))
  source: (string) @path) @import

(import_statement
  (import_clause (identifier) @name)
  source: (string) @path) @import

(import_statement
  (import_clause (namespace_import (identifier) @name))
  source: (string) @path) @import
