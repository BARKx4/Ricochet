const WORDS = [
  {
    "word": "+",
    "aliases": ["add"],
    "group": "math",
    "stack": "left:number right:number -> sum:number",
    "body": "Adds two numeric values. Integer-only addition checks overflow; any float operand promotes the result to Float.",
    "example": "20 22 +"
  },
  {
    "word": "add",
    "aliases": ["+"],
    "group": "math",
    "stack": "left:number right:number -> sum:number",
    "body": "Readable alias for `+`.",
    "example": "20 22 add"
  },
  {
    "word": "-",
    "aliases": ["subtract"],
    "group": "math",
    "stack": "left:number right:number -> difference:number",
    "body": "Subtracts the right numeric value from the left. Integer-only subtraction checks overflow; any float operand promotes the result to Float.",
    "example": "10 3 -"
  },
  {
    "word": "subtract",
    "aliases": ["-"],
    "group": "math",
    "stack": "left:number right:number -> difference:number",
    "body": "Readable alias for `-`.",
    "example": "10 3 subtract"
  },
  {
    "word": "equals",
    "aliases": ["="],
    "group": "math",
    "stack": "left:any right:any -> bool",
    "body": "Compares two Ricochet values for equality.",
    "example": "\"Ada\" \"Ada\" equals"
  },
  {
    "word": "=",
    "aliases": ["equals"],
    "group": "math",
    "stack": "left:any right:any -> bool",
    "body": "Symbol alias for `equals`.",
    "example": "42 42 ="
  },
  {
    "word": "not_equals?",
    "aliases": ["!="],
    "group": "math",
    "stack": "left:any right:any -> bool",
    "body": "Returns true when two values are not equal.",
    "example": "\"Ada\" \"Grace\" not_equals?"
  },
  {
    "word": "!=",
    "aliases": ["not_equals?"],
    "group": "math",
    "stack": "left:any right:any -> bool",
    "body": "Symbol alias for `not_equals?`.",
    "example": "1 2 !="
  },
  {
    "word": "assert_equals",
    "aliases": [],
    "group": "math",
    "stack": "actual:any expected:any ->",
    "body": "Fails the current VM run when actual and expected differ. Used by `rco test`.",
    "example": "\"Ada\" \"Ada\" assert_equals"
  },
  {
    "word": "assert",
    "aliases": [],
    "group": "math",
    "stack": "value ->",
    "body": "Consumes a truthy value or fails the current VM run.",
    "example": "$email empty? not assert"
  },
  {
    "word": "assert_true",
    "aliases": [],
    "group": "math",
    "stack": "bool ->",
    "body": "Consumes true or fails the current VM run.",
    "example": "$saved assert_true"
  },
  {
    "word": "assert_false",
    "aliases": [],
    "group": "math",
    "stack": "bool ->",
    "body": "Consumes false or fails the current VM run.",
    "example": "$deleted assert_false"
  },
  {
    "word": "assert_ok",
    "aliases": [],
    "group": "result",
    "stack": "result ->",
    "body": "Consumes an ok result or fails the current VM run.",
    "example": "User all assert_ok"
  },
  {
    "word": "assert_error",
    "aliases": [],
    "group": "result",
    "stack": "result ->",
    "body": "Consumes an error result or fails the current VM run.",
    "example": "\"Validation\" \"bad\" fail assert_error"
  },
  {
    "word": "less_than?",
    "aliases": ["<"],
    "group": "math",
    "stack": "left:number right:number -> bool",
    "body": "Numeric less-than comparison.",
    "example": "3 5 less_than?"
  },
  {
    "word": "<",
    "aliases": ["less_than?"],
    "group": "math",
    "stack": "left:number right:number -> bool",
    "body": "Symbol alias for `less_than?`.",
    "example": "3 5 <"
  },
  {
    "word": "greater_than?",
    "aliases": [">"],
    "group": "math",
    "stack": "left:number right:number -> bool",
    "body": "Numeric greater-than comparison.",
    "example": "8 4 greater_than?"
  },
  {
    "word": ">",
    "aliases": ["greater_than?"],
    "group": "math",
    "stack": "left:number right:number -> bool",
    "body": "Symbol alias for `greater_than?`.",
    "example": "8 4 >"
  },
  {
    "word": "less_or_equals?",
    "aliases": ["<="],
    "group": "math",
    "stack": "left:number right:number -> bool",
    "body": "Numeric less-than-or-equal comparison.",
    "example": "5 5 less_or_equals?"
  },
  {
    "word": "<=",
    "aliases": ["less_or_equals?"],
    "group": "math",
    "stack": "left:number right:number -> bool",
    "body": "Symbol alias for `less_or_equals?`.",
    "example": "5 5 <="
  },
  {
    "word": "greater_or_equals?",
    "aliases": [">="],
    "group": "math",
    "stack": "left:number right:number -> bool",
    "body": "Numeric greater-than-or-equal comparison.",
    "example": "5 5 greater_or_equals?"
  },
  {
    "word": ">=",
    "aliases": ["greater_or_equals?"],
    "group": "math",
    "stack": "left:number right:number -> bool",
    "body": "Symbol alias for `greater_or_equals?`.",
    "example": "5 5 >="
  },
  {
    "word": "swap",
    "aliases": [],
    "group": "stack",
    "stack": "a b -> b a",
    "body": "Swaps the top two stack values.",
    "example": "$ctx \"home/index\" swap view"
  },
  {
    "word": "dup",
    "aliases": [],
    "group": "stack",
    "stack": "a -> a a",
    "body": "Duplicates the top stack value.",
    "example": "User all dup ok?"
  },
  {
    "word": "drop",
    "aliases": [],
    "group": "stack",
    "stack": "a ->",
    "body": "Removes the top stack value.",
    "example": "self name.get dup nil? if drop self email.get end"
  },
  {
    "word": "over",
    "aliases": [],
    "group": "stack",
    "stack": "a b -> a b a",
    "body": "Copies the second stack value to the top.",
    "example": "1 2 over"
  },
  {
    "word": "rot",
    "aliases": [],
    "group": "stack",
    "stack": "a b c -> b c a",
    "body": "Rotates the third stack value to the top.",
    "example": "1 2 3 rot"
  },
  {
    "word": "nil",
    "aliases": [],
    "group": "data",
    "stack": "-> nil",
    "body": "Literal nil value. It is falsey in ordinary conditions.",
    "example": "nil nil?"
  },
  {
    "word": "true",
    "aliases": [],
    "group": "data",
    "stack": "-> bool",
    "body": "Literal boolean true.",
    "example": "true if \"yes\" else \"no\" end"
  },
  {
    "word": "false",
    "aliases": [],
    "group": "data",
    "stack": "-> bool",
    "body": "Literal boolean false.",
    "example": "false if \"yes\" else \"no\" end"
  },
  {
    "word": "array",
    "aliases": [],
    "group": "data",
    "stack": "name?:string -> | -> array",
    "body": "With a name on top, declares a mutable array variable. With no name, pushes a new empty array. Anonymous construction is also available as `Array new`.",
    "example": "users array\n$users \"Ada\" push drop"
  },
  {
    "word": "map",
    "aliases": [],
    "group": "data",
    "stack": "name?:string -> | -> map",
    "body": "With a name on top, declares a mutable map variable. With no name, pushes a new empty map. Anonymous construction is also available as `Map new`.",
    "example": "settings map\n$settings \"theme\" \"dark\" put drop"
  },
  {
    "word": "var",
    "aliases": [],
    "group": "data",
    "stack": "value? name:string ->",
    "body": "Declares a variable in the current scope. If a value is below the name, that value becomes the initial value; otherwise it starts as nil. Function and method locals refresh within the active call frame, while top-level declarations remain shared.",
    "example": "\"Ada\" name var"
  },
  {
    "word": "get",
    "aliases": [],
    "group": "data",
    "stack": "name:string -> value",
    "body": "Reads a variable by name string. Prefer `$name` for ordinary variable reads in new code; keep `get` for dynamic by-name reads. For map/object keyed access, use `container key at`; for generated object accessors, use `field.get` selectors.",
    "example": "\"name\" get\n$request \"method\" at\nuser email.get"
  },
  {
    "word": "set",
    "aliases": [],
    "group": "data",
    "stack": "value name:string ->",
    "body": "Updates an existing variable. For generated object accessors, use `field.set` selectors.",
    "example": "\"Ada\" name set\n\"ada@example.com\" user email.set"
  },
  {
    "word": "empty?",
    "aliases": [],
    "group": "data",
    "stack": "string|array|map -> bool",
    "body": "Returns true for empty strings, arrays, and maps.",
    "example": "array empty?"
  },
  {
    "word": "nil?",
    "aliases": [],
    "group": "data",
    "stack": "value -> bool",
    "body": "Returns true only for nil.",
    "example": "self name.get nil?"
  },
  {
    "word": "Subclass",
    "aliases": [],
    "group": "oop",
    "stack": "className:string superclass:class|string ->",
    "body": "Creates a class. Static declarations use `User Model Subclass`; runtime declarations can use strings and variables.",
    "example": "User Model Subclass\nend\n\n\"Widget\" \"Object\" Subclass"
  },
  {
    "word": "Field",
    "aliases": [],
    "group": "oop",
    "stack": "class:string|class fieldName:string ->",
    "body": "Adds storage to a runtime class without generating accessor selectors. Inside a class body, use a string field name followed by `Field`.",
    "example": "\"email\" Field\nUser \"email\" Field"
  },
  {
    "word": "Accessor",
    "aliases": [],
    "group": "oop",
    "stack": "class:string|class fieldName:string ->",
    "body": "Adds storage to a class and generates `field.get` and `field.set` selectors.",
    "example": "\"email\" Accessor\nUser \"email\" Accessor"
  },
  {
    "word": "Table",
    "aliases": [],
    "group": "oop",
    "stack": "class:string|class tableName:string ->",
    "body": "Sets a model table name. Inside a class body, use a string table name followed by `Table`.",
    "example": "\"users\" Table\nUser \"users\" Table"
  },
  {
    "word": "new",
    "aliases": [],
    "group": "oop",
    "stack": "class:string|class -> instance",
    "body": "Instantiates a class and initializes inherited fields to nil.",
    "example": "User new\n\"User\" new"
  },
  {
    "word": "self",
    "aliases": [],
    "group": "oop",
    "stack": "-> receiver",
    "body": "Pushes the current method receiver.",
    "example": "self email.get"
  },
  {
    "word": "Method",
    "aliases": [],
    "group": "oop",
    "stack": "block methodName:string ->",
    "body": "Installs a bytecode method inside the current class body. Top-level methods are not supported.",
    "example": "[ self email.get ] \"displayName\" Method"
  },
  {
    "word": "send",
    "aliases": [],
    "group": "oop",
    "stack": "receiver methodName:string -> result",
    "body": "Calls a method whose name is a string on the stack.",
    "example": "user \"displayName\" send"
  },
  {
    "word": "field.get / field.set",
    "aliases": ["postfix selector"],
    "group": "oop",
    "stack": "receiver -> value | value receiver -> updatedReceiver",
    "body": "Generated accessor selectors are ordinary postfix method calls on the receiver.",
    "example": "user email.get\n\"ada@example.com\" user email.set"
  },
  {
    "word": "function",
    "aliases": [],
    "group": "control",
    "stack": "declaration",
    "body": "Declares a top-level function. Optional args metadata can precede the name.",
    "example": "( left right -> Number ) sum function\n  $left $right +\nend"
  },
  {
    "word": "Macro",
    "aliases": [],
    "group": "control",
    "stack": "declaration",
    "body": "Declares a beta compile-time macro. Macro names are string literals, macro bodies run in a restricted evaluator, static imports can expose public macros, `quote_ast` emits expression AST, and `quote_items` emits whole item rows including expression rows, class-body rows, and top-level declaration rows such as `[ body ] \"name\" function` or `[ body-items ] Name Superclass Subclass`.",
    "example": "\"accessors\" Macro\n  [\n    [\n      \"email\" Accessor\n      \"name\" Accessor\n    ] quote_items\n  ]\nend\n\nUser Model Subclass\n  \"accessors\" macro_call\nend\n\n\"make_greet\" Macro\n  [\n    [\n      [ \"hi\" println ] \"greet\" function\n    ] quote_items\n  ]\nend"
  },
  {
    "word": "return",
    "aliases": [],
    "group": "control",
    "stack": "value -> returns value",
    "body": "Returns early from the current bytecode function or method.",
    "example": "self name.get return"
  },
  {
    "word": "if",
    "aliases": ["else", "end"],
    "group": "control",
    "stack": "condition -> branch result",
    "body": "Starts a postfix conditional. Result values require an explicit `ok?` before use as conditions.",
    "example": "true if \"yes\" else \"no\" end"
  },
  {
    "word": "call",
    "aliases": [],
    "group": "control",
    "stack": "block -> result",
    "body": "Executes a first-class block value.",
    "example": "[ \"ok\" ] call"
  },
  {
    "word": "spawn",
    "aliases": [],
    "group": "control",
    "stack": "block -> task",
    "body": "Creates a first-class task value from a block and starts running it on a background worker. The current VM environment is captured when the task is spawned.",
    "example": "[ 40 2 + ] spawn"
  },
  {
    "word": "await",
    "aliases": [],
    "group": "control",
    "stack": "task -> result",
    "body": "Waits for a spawned task if needed and returns its result. Completed handles can be awaited again from their cached value.",
    "example": "$task await"
  },
  {
    "word": "await_all",
    "aliases": [],
    "group": "control",
    "stack": "array|list -> array",
    "body": "Awaits an array or list of task handles and returns their results in input order. Completed handles reuse their cached values.",
    "example": "$handles await_all"
  },
  {
    "word": "release_task",
    "aliases": [],
    "group": "control",
    "stack": "task -> bool",
    "body": "Releases an awaited completed or failed task handle from the current VM's retained task table. Running tasks must be awaited before release.",
    "example": "$task await\n$task release_task"
  },
  {
    "word": "tasks",
    "aliases": [],
    "group": "inspect",
    "stack": "-> array",
    "body": "Returns metadata maps for running spawned tasks in the current VM. Individual task handles expose id/status/predicate metadata through `info`.",
    "example": "tasks count"
  },
  {
    "word": "task_status",
    "aliases": ["tasks"],
    "group": "inspect",
    "stack": "task -> string",
    "body": "Returns `running`, `completed`, `failed`, or `consumed` for a task handle.",
    "example": "$task task_status"
  },
  {
    "word": "while",
    "aliases": ["end"],
    "group": "control",
    "stack": "conditionExpression -> repeated body",
    "body": "Re-executes the condition expression before every iteration and runs the body while it remains truthy.",
    "example": "$count 10 < while\n  $count 1 + count set\nend"
  },
  {
    "word": "break",
    "aliases": [],
    "group": "control",
    "stack": "-> exits nearest loop",
    "body": "Exits the nearest enclosing `while`. Using it outside a loop is a compile error.",
    "example": "done? if break end"
  },
  {
    "word": "continue",
    "aliases": [],
    "group": "control",
    "stack": "-> rechecks nearest loop",
    "body": "Jumps to the condition of the nearest enclosing `while`. Using it outside a loop is a compile error.",
    "example": "skip? if continue end"
  },
  {
    "word": "println",
    "aliases": [],
    "group": "web",
    "stack": "value ->",
    "body": "Records a line of output. `rco run` prints captured output before the final stack.",
    "example": "\"Hello Ricochet\" println"
  },
  {
    "word": "view",
    "aliases": [],
    "group": "web",
    "stack": "viewName:string -> action | viewName:string ctx:any -> action",
    "body": "Builds a controller action result map for rendering a view.",
    "example": "$ctx \"home/index\" swap view"
  },
  {
    "word": "text",
    "aliases": [],
    "group": "web",
    "stack": "body:string -> action | body:string ctx:any -> action",
    "body": "Builds a plain text controller response.",
    "example": "\"pong\" text"
  },
  {
    "word": "json",
    "aliases": [],
    "group": "web",
    "stack": "body:any -> action",
    "body": "Builds a JSON controller response from nil, bool, number, float, string, array, or map values.",
    "example": "payload map\n$payload \"ok\" true put drop\n$payload json"
  },
  {
    "word": "redirect",
    "aliases": [],
    "group": "web",
    "stack": "location:string -> action | location:string ctx:any -> action",
    "body": "Builds an HTTP redirect controller response. The server defaults to HTTP 302 unless a later `status` word changes it.",
    "example": "\"/dashboard\" redirect"
  },
  {
    "word": "status",
    "aliases": [],
    "group": "web",
    "stack": "action status:number -> action",
    "body": "Sets the HTTP status code on a controller action result map.",
    "example": "\"created\" text 201 status"
  },
  {
    "word": "header",
    "aliases": [],
    "group": "web",
    "stack": "action name:string value:string -> action",
    "body": "Adds a response header to a controller action result map.",
    "example": "\"pong\" text \"x-ricochet\" \"yes\" header"
  },
  {
    "word": "route",
    "aliases": ["GET", "POST", "PUT", "PATCH", "DELETE"],
    "group": "web",
    "stack": "route-file declaration",
    "body": "Route parser operator. Use five tokens per line: method, path, controller, action, `route`.",
    "example": "GET \"/\" HomeController \"index\" route"
  },
  {
    "word": "ok?",
    "aliases": [],
    "group": "result",
    "stack": "result -> bool",
    "body": "Returns true for ok results and false for error results.",
    "example": "User all dup ok? if value else error end"
  },
  {
    "word": "value",
    "aliases": [],
    "group": "result",
    "stack": "okResult -> value",
    "body": "Unwraps an ok result. Fails loudly if the result is an error.",
    "example": "User all dup ok? if value users var end"
  },
  {
    "word": "error",
    "aliases": [],
    "group": "result",
    "stack": "errorResult -> map",
    "body": "Unwraps an error result into a map with `kind` and `message`.",
    "example": "User all dup ok? if value else error \"message\" at end"
  },
  {
    "word": "all",
    "aliases": ["Active Record"],
    "group": "web",
    "stack": "ModelClass -> result(array)",
    "body": "Active Record class method installed for mapped model classes.",
    "example": "User all"
  },
  {
    "word": "find_record",
    "aliases": ["Active Record"],
    "group": "web",
    "stack": "id ModelClass -> result(record|nil) | modelName:string id:any DatabaseCapability -> result(record|nil)",
    "body": "Finds a mapped database row by id.",
    "example": "42 User find_record\n\"User\" 42 $db find_record"
  },
  {
    "word": "default_page",
    "aliases": ["Active Record"],
    "group": "web",
    "stack": "ModelClass -> result(array) | modelName:string DatabaseCapability -> result(array)",
    "body": "Loads the v1 beta default list page: up to 50 rows, ordered by `id asc` when the model maps an `id` field, otherwise the first bounded page.",
    "example": "User default_page\n\"User\" $db default_page"
  },
  {
    "word": "where",
    "aliases": ["Active Record"],
    "group": "web",
    "stack": "field:string value:any ModelClass -> result(array)",
    "body": "Runs an equality query against a mapped model field.",
    "example": "\"email\" \"ada@example.com\" User where"
  },
  {
    "word": "limit",
    "aliases": ["Active Record"],
    "group": "web",
    "stack": "count:number ModelClass -> result(array)",
    "body": "Loads at most `count` rows from a mapped model class.",
    "example": "10 User limit"
  },
  {
    "word": "count_records",
    "aliases": ["Active Record"],
    "group": "web",
    "stack": "ModelClass -> result(number) | modelName:string DatabaseCapability -> result(number)",
    "body": "Counts rows for a mapped model class.",
    "example": "User count_records\n\"User\" $db count_records"
  },
  {
    "word": "first_record",
    "aliases": ["Active Record"],
    "group": "web",
    "stack": "ModelClass -> result(record|nil) | modelName:string DatabaseCapability -> result(record|nil)",
    "body": "Loads the first row for a mapped model class.",
    "example": "User first_record\n\"User\" $db first_record"
  },
  {
    "word": "exists?",
    "aliases": ["Active Record"],
    "group": "web",
    "stack": "id ModelClass -> result(bool)",
    "body": "Checks whether a mapped row exists by id.",
    "example": "1 User exists?"
  },
  {
    "word": "insert",
    "aliases": ["Active Record"],
    "group": "web",
    "stack": "attributes:map ModelClass -> result(record)",
    "body": "Inserts a row using mapped non-id fields and returns the inserted record.",
    "example": "attributes map\n$attributes \"email\" \"ada@example.com\" put drop\n$attributes User insert"
  },
  {
    "word": "update",
    "aliases": ["Active Record"],
    "group": "web",
    "stack": "id attributes:map ModelClass -> result(record)",
    "body": "Updates a row by id using mapped non-id fields and returns the updated record.",
    "example": "updates map\n$updates \"email\" \"grace@example.com\" put drop\n42 $updates User update"
  },
  {
    "word": "transaction",
    "aliases": ["application transaction", "DatabaseCapability"],
    "group": "web",
    "stack": "body:block DatabaseCapability -> result",
    "body": "Runs the block in one database transaction. The block must return a Result: Ok commits, Err rolls back, and the original Result is preserved unless commit or rollback itself fails. An uncaught VM error triggers best-effort rollback when the database session is discarded.",
    "example": "[\n  attributes map\n  $attributes \"email\" \"ada@example.com\" put drop\n  $attributes User insert\n] $db transaction"
  },
  {
    "word": "begin",
    "aliases": ["application transaction", "DatabaseCapability"],
    "group": "web",
    "stack": "DatabaseCapability -> result(bool)",
    "body": "Begins a manual database transaction on the MVC database capability. A second begin fails; use savepoint for nested work.",
    "example": "$db begin"
  },
  {
    "word": "commit",
    "aliases": ["application transaction", "DatabaseSavepoint"],
    "group": "web",
    "stack": "DatabaseCapability|DatabaseSavepoint -> result(bool)",
    "body": "Commits the active database transaction. On a DatabaseSavepoint receiver, releases only that savepoint; the outer transaction remains active.",
    "example": "$db commit\n$checkpoint commit"
  },
  {
    "word": "rollback",
    "aliases": ["application transaction", "DatabaseSavepoint"],
    "group": "web",
    "stack": "DatabaseCapability|DatabaseSavepoint -> result(bool)",
    "body": "Rolls back the active database transaction. On a DatabaseSavepoint receiver, rolls back to and releases that savepoint; the outer transaction remains active.",
    "example": "$db rollback\n$checkpoint rollback"
  },
  {
    "word": "savepoint",
    "aliases": ["application transaction", "DatabaseCapability"],
    "group": "web",
    "stack": "DatabaseCapability -> result(DatabaseSavepoint)",
    "body": "Creates a savepoint in the active transaction. Savepoints must be committed or rolled back in last-created, first-completed order.",
    "example": "$db savepoint value checkpoint var"
  },
  {
    "word": "*",
    "aliases": ["multiply"],
    "group": "math",
    "stack": "left:number right:number -> product:number",
    "body": "Multiplies two numeric values. Integer-only multiplication checks overflow; any float operand promotes the result to Float.",
    "example": "6 7 *"
  },
  {
    "word": "/",
    "aliases": ["divide"],
    "group": "math",
    "stack": "left:number right:number -> quotient:number",
    "body": "Divides two numeric values. Integer-only division stays integer; any float operand promotes the result to Float. Division by zero fails loudly and preserves operands.",
    "example": "22 5 /"
  },
  {
    "word": "%",
    "aliases": ["modulo"],
    "group": "math",
    "stack": "left:number right:number -> remainder:number",
    "body": "Integer remainder. Modulo by zero fails loudly and preserves operands.",
    "example": "22 5 %"
  },
  {
    "word": "negate",
    "aliases": [],
    "group": "math",
    "stack": "number -> number",
    "body": "Negates a numeric value with integer overflow checks.",
    "example": "5 negate"
  },
  {
    "word": "abs",
    "aliases": [],
    "group": "math",
    "stack": "number -> number",
    "body": "Absolute value with integer overflow checks.",
    "example": "0 5 - abs"
  },
  {
    "word": "min",
    "aliases": [],
    "group": "math",
    "stack": "a:number b:number -> number",
    "body": "Returns the smaller numeric value, promoting to Float when either operand is Float.",
    "example": "3 7 min"
  },
  {
    "word": "max",
    "aliases": [],
    "group": "math",
    "stack": "a:number b:number -> number",
    "body": "Returns the larger numeric value, promoting to Float when either operand is Float.",
    "example": "3 7 max"
  },
  {
    "word": "clamp",
    "aliases": [],
    "group": "math",
    "stack": "value:number min:number max:number -> number",
    "body": "Clamps a numeric value into an inclusive range, promoting to Float when any operand is Float.",
    "example": "15 0 10 clamp"
  },
  {
    "word": "not",
    "aliases": [],
    "group": "math",
    "stack": "value -> bool",
    "body": "Boolean negation using Ricochet truthiness. Result values must be checked with `ok?` first.",
    "example": "false not"
  },
  {
    "word": "and",
    "aliases": [],
    "group": "math",
    "stack": "left:any right:any -> bool",
    "body": "Truthiness-based boolean and.",
    "example": "true false and"
  },
  {
    "word": "or",
    "aliases": [],
    "group": "math",
    "stack": "left:any right:any -> bool",
    "body": "Truthiness-based boolean or.",
    "example": "true false or"
  },
  {
    "word": "nip",
    "aliases": [],
    "group": "stack",
    "stack": "a b -> b",
    "body": "Drops the second stack value and keeps the top.",
    "example": "1 2 nip"
  },
  {
    "word": "tuck",
    "aliases": [],
    "group": "stack",
    "stack": "a b -> b a b",
    "body": "Copies the top value underneath the second value.",
    "example": "1 2 tuck"
  },
  {
    "word": "pick",
    "aliases": [],
    "group": "stack",
    "stack": "index:number -> copiedValue",
    "body": "Copies a value by zero-based depth from the top of the stack.",
    "example": "10 20 30 2 pick"
  },
  {
    "word": "roll",
    "aliases": [],
    "group": "stack",
    "stack": "index:number -> movedValue",
    "body": "Moves a value by zero-based depth to the top of the stack.",
    "example": "10 20 30 2 roll"
  },
  {
    "word": "depth",
    "aliases": [],
    "group": "stack",
    "stack": "-> count:number",
    "body": "Pushes the current stack depth.",
    "example": "1 2 depth"
  },
  {
    "word": "clear",
    "aliases": [],
    "group": "stack",
    "stack": "many ->",
    "body": "Clears the stack.",
    "example": "1 2 3 clear"
  },
  {
    "word": "list",
    "aliases": [],
    "group": "collection",
    "stack": "name?:string -> | -> list",
    "body": "Declares a mutable list variable when given a name, otherwise pushes an empty list.",
    "example": "queue list\n$queue 1 push drop"
  },
  {
    "word": "Set",
    "aliases": ["Set new"],
    "group": "collection",
    "stack": "name?:string -> | -> Class(Set)",
    "body": "With a name on top, declares a mutable set variable. With no name, pushes the Set class for `Set new`.",
    "example": "tags Set\n$tags \"rco\" push drop"
  },
  {
    "word": "range",
    "aliases": [],
    "group": "collection",
    "stack": "start:number end:number -> array",
    "body": "Builds a half-open numeric range. Descending ranges count downward.",
    "example": "0 6 range"
  },
  {
    "word": "push",
    "aliases": [],
    "group": "collection",
    "stack": "collection value -> sameCollection",
    "body": "Mutates an array, list, or set in place and returns the same collection for chaining.",
    "example": "$users \"Ada\" push \"Grace\" push drop"
  },
  {
    "word": "put",
    "aliases": [],
    "group": "collection",
    "stack": "map key:string value:any -> sameMap",
    "body": "Mutates a map in place and returns the same map for chaining.",
    "example": "$settings \"theme\" \"dark\" put drop"
  },
  {
    "word": "insert_at",
    "aliases": [],
    "group": "collection",
    "stack": "index:number value:any array|list -> sameCollection",
    "body": "Inserts at a zero-based index.",
    "example": "$users 1 \"Lin\" insert_at drop"
  },
  {
    "word": "remove",
    "aliases": [],
    "group": "collection",
    "stack": "value:any collection -> sameCollection | key:string map -> sameMap",
    "body": "Removes a matching value from arrays/lists/sets or a key from maps.",
    "example": "$settings \"theme\" remove drop"
  },
  {
    "word": "remove_at",
    "aliases": [],
    "group": "collection",
    "stack": "index:number array|list -> sameCollection",
    "body": "Removes the value at a zero-based index.",
    "example": "$users 0 remove_at drop"
  },
  {
    "word": "clear_items",
    "aliases": [],
    "group": "collection",
    "stack": "collection -> sameCollection",
    "body": "Clears a mutable collection in place.",
    "example": "$users clear_items drop"
  },
  {
    "word": "at",
    "aliases": [],
    "group": "collection",
    "stack": "index:number string|array|list -> value | key:string map -> value",
    "body": "Reads an indexed value, character, or map entry. Missing values produce nil.",
    "example": "$users 0 at\n$settings \"theme\" at"
  },
  {
    "word": "count",
    "aliases": ["length"],
    "group": "collection",
    "stack": "string|collection -> number",
    "body": "Counts characters for strings or items for collections.",
    "example": "$users count"
  },
  {
    "word": "first",
    "aliases": [],
    "group": "collection",
    "stack": "string|array|list|set -> value|nil",
    "body": "Returns the first character or item, or nil for an empty receiver.",
    "example": "$users first"
  },
  {
    "word": "last",
    "aliases": [],
    "group": "collection",
    "stack": "string|array|list|set -> value|nil",
    "body": "Returns the last character or item, or nil for an empty receiver.",
    "example": "$users last"
  },
  {
    "word": "take",
    "aliases": [],
    "group": "collection",
    "stack": "count:number string|array|list|set -> sameKind",
    "body": "Returns the first count characters or items.",
    "example": "$users 2 take"
  },
  {
    "word": "skip",
    "aliases": [],
    "group": "collection",
    "stack": "count:number string|array|list|set -> sameKind",
    "body": "Returns characters or items after the first count entries.",
    "example": "$users 1 skip"
  },
  {
    "word": "reverse",
    "aliases": [],
    "group": "collection",
    "stack": "string|array|list|set -> sameKind",
    "body": "Returns characters or items in reverse order.",
    "example": "$users reverse"
  },
  {
    "word": "has?",
    "aliases": [],
    "group": "collection",
    "stack": "value:any collection -> bool | key:string map -> bool",
    "body": "Checks membership or map-key presence.",
    "example": "$settings \"theme\" has?"
  },
  {
    "word": "keys",
    "aliases": [],
    "group": "collection",
    "stack": "map -> array",
    "body": "Returns map keys as an array of strings.",
    "example": "$settings keys"
  },
  {
    "word": "values",
    "aliases": [],
    "group": "collection",
    "stack": "map -> array",
    "body": "Returns map values as an array.",
    "example": "$settings values"
  },
  {
    "word": "each",
    "aliases": [],
    "group": "collection",
    "stack": "block collection -> sameCollection",
    "body": "Runs a block for each item. Map blocks receive key then value.",
    "example": "[ println ] $users each drop"
  },
  {
    "word": "transform",
    "aliases": [],
    "group": "collection",
    "stack": "block collection -> array",
    "body": "Maps each item through a block and returns an array.",
    "example": "[ 2 * ] $numbers transform"
  },
  {
    "word": "select",
    "aliases": [],
    "group": "collection",
    "stack": "block collection -> collection",
    "body": "Keeps items whose block result is truthy.",
    "example": "[ 4 > ] $numbers select"
  },
  {
    "word": "reduce",
    "aliases": [],
    "group": "collection",
    "stack": "initial:any block array|list|set -> value",
    "body": "Reduces a sequence by calling the block with accumulator then item.",
    "example": "0 [ + ] $numbers reduce"
  },
  {
    "word": "find",
    "aliases": [],
    "group": "collection",
    "stack": "block collection -> value|nil",
    "body": "Returns the first collection item whose block result is truthy.",
    "example": "[ 8 = ] $numbers find"
  },
  {
    "word": "any?",
    "aliases": [],
    "group": "collection",
    "stack": "block collection -> bool",
    "body": "Returns true if any item matches.",
    "example": "[ 10 = ] $numbers any?"
  },
  {
    "word": "all?",
    "aliases": [],
    "group": "collection",
    "stack": "block collection -> bool",
    "body": "Returns true if every item matches.",
    "example": "[ 0 > ] $numbers all?"
  },
  {
    "word": "join",
    "aliases": [],
    "group": "collection",
    "stack": "separator:string collection -> string",
    "body": "Joins a collection of displayable values into a string.",
    "example": "$users \", \" join"
  },
  {
    "word": "trim",
    "aliases": [],
    "group": "string",
    "stack": "string -> string",
    "body": "Trims leading and trailing whitespace.",
    "example": "\" Ada \" trim"
  },
  {
    "word": "trim_start",
    "aliases": [],
    "group": "string",
    "stack": "string -> string",
    "body": "Trims leading whitespace.",
    "example": "\"  Ada\" trim_start"
  },
  {
    "word": "trim_end",
    "aliases": [],
    "group": "string",
    "stack": "string -> string",
    "body": "Trims trailing whitespace.",
    "example": "\"Ada  \" trim_end"
  },
  {
    "word": "blank?",
    "aliases": [],
    "group": "string",
    "stack": "string -> bool",
    "body": "Returns true when a string is empty or only whitespace.",
    "example": "\"  \" blank?"
  },
  {
    "word": "slice",
    "aliases": [],
    "group": "string",
    "stack": "start:number count:number string -> string",
    "body": "Returns count characters starting at start.",
    "example": "\"ricochet\" 2 4 slice"
  },
  {
    "word": "index_of",
    "aliases": [],
    "group": "string",
    "stack": "needle:string string -> number|nil",
    "body": "Returns the first character index for a substring, or nil.",
    "example": "\"ricochet\" \"co\" index_of"
  },
  {
    "word": "last_index_of",
    "aliases": [],
    "group": "string",
    "stack": "needle:string string -> number|nil",
    "body": "Returns the last character index for a substring, or nil.",
    "example": "\"ricochet\" \"c\" last_index_of"
  },
  {
    "word": "repeat",
    "aliases": [],
    "group": "string",
    "stack": "count:number string -> string",
    "body": "Repeats a string count times.",
    "example": "\"ha\" 3 repeat"
  },
  {
    "word": "lines",
    "aliases": [],
    "group": "string",
    "stack": "string -> array",
    "body": "Splits a string into lines.",
    "example": "\"a\\nb\" lines"
  },
  {
    "word": "chars",
    "aliases": [],
    "group": "string",
    "stack": "string -> array",
    "body": "Splits a string into one-character strings.",
    "example": "\"cat\" chars"
  },
  {
    "word": "split",
    "aliases": [],
    "group": "string",
    "stack": "separator:string string -> array",
    "body": "Splits a string.",
    "example": "\"Ada,Grace\" \",\" split"
  },
  {
    "word": "replace",
    "aliases": [],
    "group": "string",
    "stack": "needle:string replacement:string string -> string",
    "body": "Replaces all matching substrings in a string.",
    "example": "\"telnet era\" \"telnet\" \"web\" replace"
  },
  {
    "word": "contains?",
    "aliases": [],
    "group": "string",
    "stack": "needle:string string -> bool",
    "body": "Checks substring presence.",
    "example": "\"Ricochet\" \"co\" contains?"
  },
  {
    "word": "starts_with?",
    "aliases": [],
    "group": "string",
    "stack": "prefix:string string -> bool",
    "body": "Checks a string prefix.",
    "example": "\"Ricochet\" \"Rico\" starts_with?"
  },
  {
    "word": "ends_with?",
    "aliases": [],
    "group": "string",
    "stack": "suffix:string string -> bool",
    "body": "Checks a string suffix.",
    "example": "\"Ricochet\" \"chet\" ends_with?"
  },
  {
    "word": "uppercase",
    "aliases": [],
    "group": "string",
    "stack": "string -> string",
    "body": "Converts to uppercase.",
    "example": "\"ricochet\" uppercase"
  },
  {
    "word": "lowercase",
    "aliases": [],
    "group": "string",
    "stack": "string -> string",
    "body": "Converts to lowercase.",
    "example": "\"RICOCHET\" lowercase"
  },
  {
    "word": "concat",
    "aliases": [],
    "group": "string",
    "stack": "suffix:string string -> string",
    "body": "Concatenates two strings.",
    "example": "\"Rico\" \"chet\" concat"
  },
  {
    "word": "to_number",
    "aliases": [],
    "group": "math",
    "stack": "value -> result(number)",
    "body": "Checked alias for `to_integer`. Converts an integer string, integer Number, or integral Float to a signed 64-bit Number.",
    "example": "\"42\" to_number value"
  },
  {
    "word": "to_integer",
    "aliases": ["to_number", "to_bigint"],
    "group": "math",
    "stack": "value -> result(number)",
    "body": "Converts an integer string, integer Number, or integral Float to Ricochet's signed 64-bit Number. Non-integral floats and out-of-range values return Result errors.",
    "example": "12.0 to_integer value"
  },
  {
    "word": "to_bigint",
    "aliases": ["to_integer", "to_number"],
    "group": "math",
    "stack": "value -> result(number)",
    "body": "Checked signed 64-bit integer conversion. Ricochet's Number storage is already i64-sized.",
    "example": "\"9223372036854775807\" to_bigint value"
  },
  {
    "word": "to_int",
    "aliases": [],
    "group": "math",
    "stack": "value -> result(number)",
    "body": "Checked signed 32-bit integer conversion (-2147483648..2147483647).",
    "example": "2147483647 to_int value"
  },
  {
    "word": "to_mediumint",
    "aliases": [],
    "group": "math",
    "stack": "value -> result(number)",
    "body": "Checked signed 24-bit integer conversion (-8388608..8388607).",
    "example": "8388607 to_mediumint value"
  },
  {
    "word": "to_smallint",
    "aliases": [],
    "group": "math",
    "stack": "value -> result(number)",
    "body": "Checked signed 16-bit integer conversion (-32768..32767).",
    "example": "32767 to_smallint value"
  },
  {
    "word": "to_tinyint",
    "aliases": [],
    "group": "math",
    "stack": "value -> result(number)",
    "body": "Checked signed 8-bit integer conversion (-128..127).",
    "example": "127 to_tinyint value"
  },
  {
    "word": "to_bit",
    "aliases": [],
    "group": "math",
    "stack": "value -> result(number)",
    "body": "Converts bools or exact integers in the 0..1 range to a bit Number.",
    "example": "true to_bit value"
  },
  {
    "word": "to_unsigned_int",
    "aliases": [],
    "group": "math",
    "stack": "value -> result(number)",
    "body": "Checked unsigned 32-bit integer conversion (0..4294967295).",
    "example": "4294967295 to_unsigned_int value"
  },
  {
    "word": "to_unsigned_mediumint",
    "aliases": [],
    "group": "math",
    "stack": "value -> result(number)",
    "body": "Checked unsigned 24-bit integer conversion (0..16777215).",
    "example": "16777215 to_unsigned_mediumint value"
  },
  {
    "word": "to_unsigned_smallint",
    "aliases": [],
    "group": "math",
    "stack": "value -> result(number)",
    "body": "Checked unsigned 16-bit integer conversion (0..65535).",
    "example": "65535 to_unsigned_smallint value"
  },
  {
    "word": "to_unsigned_tinyint",
    "aliases": [],
    "group": "math",
    "stack": "value -> result(number)",
    "body": "Checked unsigned 8-bit integer conversion (0..255).",
    "example": "255 to_unsigned_tinyint value"
  },
  {
    "word": "to_unsigned_bigint",
    "aliases": [],
    "group": "math",
    "stack": "value -> result(number)",
    "body": "Checked non-negative Number conversion using Ricochet's signed i64 storage (0..9223372036854775807). Full u64 storage is outside the v1 beta storage model.",
    "example": "42 to_unsigned_bigint value"
  },
  {
    "word": "to_float",
    "aliases": ["to_float64", "to_double", "to_real"],
    "group": "math",
    "stack": "value -> result(float)",
    "body": "Converts a numeric string, Number, or Float to a finite 64-bit Float.",
    "example": "\"1.25\" to_float value"
  },
  {
    "word": "to_float32",
    "aliases": [],
    "group": "math",
    "stack": "value -> result(float)",
    "body": "Converts to single-precision float semantics, then stores the rounded finite value as Ricochet Float.",
    "example": "1.23456789 to_float32 value"
  },
  {
    "word": "to_float64",
    "aliases": ["to_float", "to_double", "to_real"],
    "group": "math",
    "stack": "value -> result(float)",
    "body": "Checked alias for `to_float`; Ricochet Float storage is f64.",
    "example": "1 to_float64 value"
  },
  {
    "word": "to_double",
    "aliases": ["to_float", "to_float64", "to_real"],
    "group": "math",
    "stack": "value -> result(float)",
    "body": "Checked alias for `to_float`; double/real values use Ricochet's f64 Float storage.",
    "example": "\"2.5\" to_double value"
  },
  {
    "word": "to_real",
    "aliases": ["to_float", "to_float64", "to_double"],
    "group": "math",
    "stack": "value -> result(float)",
    "body": "Checked alias for `to_float`; real values use Ricochet's f64 Float storage.",
    "example": "\"2.5\" to_real value"
  },
  {
    "word": "to_string",
    "aliases": [],
    "group": "string",
    "stack": "value -> string",
    "body": "Converts any value to its display string.",
    "example": "42 to_string"
  },
  {
    "word": "json_encode",
    "aliases": [],
    "group": "string",
    "stack": "value -> string",
    "body": "Encodes nil, bool, number, float, string, array, list, set, map, or result values as JSON.",
    "example": "$settings json_encode"
  },
  {
    "word": "json_decode",
    "aliases": [],
    "group": "string",
    "stack": "string -> result(value)",
    "body": "Decodes JSON into Ricochet values.",
    "example": "\"{\\\"ok\\\":true}\" json_decode value"
  },
  {
    "word": "regex",
    "aliases": [],
    "group": "string",
    "stack": "pattern:string -> result(regex)",
    "body": "Compiles a regular expression and returns a stack result.",
    "example": "\"^[a-z0-9_-]+$\" regex value"
  },
  {
    "word": "matches?",
    "aliases": ["regex"],
    "group": "string",
    "stack": "haystack:string regex -> bool",
    "body": "Returns true when the regex matches the string.",
    "example": "\"hello-world\" $slugPattern matches?"
  },
  {
    "word": "regex_find",
    "aliases": ["regex"],
    "group": "string",
    "stack": "haystack:string regex -> map|nil",
    "body": "Returns the first regex match as a map with `text`, `start`, and `end`.",
    "example": "\"abc123\" $digits regex_find"
  },
  {
    "word": "regex_replace",
    "aliases": ["regex"],
    "group": "string",
    "stack": "haystack:string replacement:string regex -> string",
    "body": "Replaces all regex matches in a string.",
    "example": "$digits \"abc123\" \"#\" regex_replace"
  },
  {
    "word": "captures",
    "aliases": ["regex"],
    "group": "string",
    "stack": "haystack:string regex -> map|nil",
    "body": "Returns numbered and named capture groups as a map, or nil.",
    "example": "\"item-42\" $pairPattern captures"
  },
  {
    "word": "ok",
    "aliases": [],
    "group": "result",
    "stack": "value -> result",
    "body": "Wraps a value as an ok stack result.",
    "example": "42 ok"
  },
  {
    "word": "fail",
    "aliases": [],
    "group": "result",
    "stack": "kind:string message:string -> result",
    "body": "Builds an error stack result.",
    "example": "\"Validation\" \"email required\" fail"
  },
  {
    "word": "error?",
    "aliases": [],
    "group": "result",
    "stack": "result -> bool",
    "body": "Returns true for error results.",
    "example": "$result error?"
  },
  {
    "word": "unwrap_or",
    "aliases": [],
    "group": "result",
    "stack": "fallback:any result -> value",
    "body": "Returns the ok value or a fallback.",
    "example": "$maybeName \"guest\" unwrap_or"
  },
  {
    "word": "map_result",
    "aliases": [],
    "group": "result",
    "stack": "block result -> result",
    "body": "Transforms an ok value and passes error results through unchanged.",
    "example": "21 ok [ 2 * ] map_result value"
  },
  {
    "word": "and_then",
    "aliases": [],
    "group": "result",
    "stack": "block result -> result",
    "body": "Runs a block that must itself return a result when the receiver is ok.",
    "example": "$value [ ok ] and_then"
  },
  {
    "word": "result_envelope",
    "aliases": [],
    "group": "result",
    "stack": "result options:map -> map",
    "body": "Converts a Result into a shared `{ ok, data, error, meta }` map for app and API boundaries. The options map becomes `meta`; when it contains a non-empty string `capability`, error envelopes also include `error.capability`. Error `code` currently mirrors the Result kind.",
    "example": "options map\n$options \"capability\" \"workspace.read\" put drop\n\"payload\" ok $options result_envelope"
  },
  {
    "word": "print",
    "aliases": [],
    "group": "system",
    "stack": "value ->",
    "body": "Writes to captured stdout without adding a newline.",
    "example": "\"Name: \" print $name print"
  },
  {
    "word": "eprint",
    "aliases": [],
    "group": "system",
    "stack": "value ->",
    "body": "Writes to captured stderr without adding a newline.",
    "example": "\"warning\" eprint"
  },
  {
    "word": "read_line",
    "aliases": [],
    "group": "system",
    "stack": "-> string|nil",
    "body": "Reads one line from the installed input reader.",
    "example": "read_line name var"
  },
  {
    "word": "args",
    "aliases": [],
    "group": "system",
    "stack": "-> array",
    "body": "Pushes trailing CLI arguments passed after `rco run <file>`.",
    "example": "args count"
  },
  {
    "word": "env_get",
    "aliases": ["env"],
    "group": "system",
    "stack": "name:string -> result(string)",
    "body": "Reads an environment variable as a result when environment access is enabled. `env` remains a compatibility alias, but new code should use `env_get`. `--env-allow NAME` can narrow access to specific variable names.",
    "example": "\"DATABASE_URL\" env_get"
  },
  {
    "word": "env_set",
    "aliases": [],
    "group": "system",
    "stack": "name:string value:string -> result(nil)",
    "body": "Sets an environment variable in the current Ricochet process when environment access is enabled. This does not mutate the parent shell. `--env-allow NAME` can narrow writes to specific variable names.",
    "example": "\"RICOCHET_MODE\" \"dev\" env_set value drop"
  },
  {
    "word": "secret_env",
    "aliases": ["secrets"],
    "group": "system",
    "stack": "name:string -> map",
    "body": "Builds an environment-backed secret reference map without reading the secret value. Resolve it later with `secret_resolve` under the environment capability.",
    "example": "\"OPENAI_API_KEY\" secret_env"
  },
  {
    "word": "secret_literal",
    "aliases": ["secrets"],
    "group": "system",
    "stack": "value:string -> map",
    "body": "Builds a literal secret reference map for tests, fixtures, and dry-run examples. Prefer `secret_env` for real local app secrets.",
    "example": "\"dry-run-token\" secret_literal"
  },
  {
    "word": "secret_resolve",
    "aliases": ["secrets"],
    "group": "system",
    "stack": "reference:map -> result(string)",
    "body": "Resolves a secret reference. Environment-backed references use the same explicit environment capability and `--env-allow NAME` bounds as `env_get`.",
    "example": "\"OPENAI_API_KEY\" secret_env secret_resolve value"
  },
  {
    "word": "password_hash",
    "aliases": ["auth", "security"],
    "group": "system",
    "stack": "password:string -> result(string)",
    "body": "Hashes a password with Argon2id and a fresh random salt, returning a PHC-format hash string suitable for credential storage. Pair it with package-level password policy checks before accepting user passwords.",
    "example": "\"Long unique passphrase 2026\" password_hash value"
  },
  {
    "word": "password_verify",
    "aliases": ["auth", "security"],
    "group": "system",
    "stack": "password:string storedHash:string -> result(bool)",
    "body": "Verifies a password against an Argon2id PHC-format stored hash. A mismatch returns `ok(false)`; malformed or unsupported stored hashes return `PasswordHashError`.",
    "example": "$password $storedHash password_verify value"
  },
  {
    "word": "config_get",
    "aliases": ["config"],
    "group": "system",
    "stack": "config:map key-or-path:string|array -> result(value)",
    "body": "Reads a required config value from a map. A string reads one key; an array/list of strings walks nested maps and returns a `ConfigError` result if the path is missing.",
    "example": "path array\n$path \"provider\" push drop\n$path \"token\" push drop\n$config $path config_get value"
  },
  {
    "word": "cwd",
    "aliases": [],
    "group": "system",
    "stack": "-> result(string)",
    "body": "Returns the current working directory.",
    "example": "cwd value"
  },
  {
    "word": "runtime_capabilities",
    "aliases": ["capabilities"],
    "group": "system",
    "stack": "-> map",
    "body": "Returns a map describing enabled host capabilities such as filesystem, workspace, HTTP, sockets, process, PTY, approval, environment, sleep, TUI, and webview. Environment entries include an `allowlist` array when access is name-bounded; sockets include allowed hosts and retained TCP/WebSocket counts; process entries include the cwd root used by process and PTY launches.",
    "example": "runtime_capabilities \"environment\" at \"allowlist\" at"
  },
  {
    "word": "process_spawn",
    "aliases": ["process"],
    "group": "system",
    "stack": "command:string args:array options:map -> result(map)",
    "body": "Runs a direct child process to completion when the process capability is enabled. Options include `cwd`, `stdin`, `timeout_ms`, `clear_env`, `env`, `stdout_max_bytes`, and `stderr_max_bytes`; `cwd` is bounded by `--process-root` when configured, then by `--fs-root`. The result map includes `success`, `status`, `stdout`, `stderr`, `stdout_truncated`, and `stderr_truncated`.",
    "example": "args array\noptions map\n\"git\" $args $options process_spawn value"
  },
  {
    "word": "process_spawn_task",
    "aliases": ["process"],
    "group": "system",
    "stack": "command:string args:array options:map -> task",
    "body": "Starts `process_spawn` on a task worker. Await the task to receive the same result map returned by `process_spawn`.",
    "example": "args array\noptions map\n\"git\" $args $options process_spawn_task await value"
  },
  {
    "word": "process_start",
    "aliases": [],
    "group": "system",
    "stack": "command:string args:array options:map -> result(map)",
    "body": "Starts a direct child process as a long-running job when the process capability is enabled. Options include `stdin_open`; when true, the returned snapshot reports `stdin_open` and the job can receive input through `process_write`. The returned snapshot also includes `id`, `status`, `running`, `success`, output lengths, truncation flags, timeout state, and cancellation state. Retained jobs are capped; release completed jobs with `process_release`.",
    "example": "args array\noptions map\n\"git\" $args $options process_start value"
  },
  {
    "word": "process_jobs",
    "aliases": [],
    "group": "system",
    "stack": "-> array",
    "body": "Returns snapshots for the current VM host's retained process jobs. MVC servers share this registry across request VMs when started with `--allow-process`.",
    "example": "process_jobs count"
  },
  {
    "word": "process_job",
    "aliases": [],
    "group": "system",
    "stack": "id:number -> result(map)",
    "body": "Returns a snapshot for a retained process job, or a `ProcessNotFound` result when the id is unknown.",
    "example": "$job \"id\" at process_job value"
  },
  {
    "word": "process_cancel",
    "aliases": [],
    "group": "system",
    "stack": "id:number -> result(map)",
    "body": "Requests cancellation for a running process job and returns the latest snapshot. Completed jobs remain inspectable.",
    "example": "$job \"id\" at process_cancel value"
  },
  {
    "word": "process_release",
    "aliases": [],
    "group": "system",
    "stack": "id:number -> result(bool)",
    "body": "Removes a completed retained process job from the host registry. Running jobs return `ProcessRunning`; cancel or wait for completion first.",
    "example": "$job \"id\" at process_release value"
  },
  {
    "word": "process_write",
    "aliases": [],
    "group": "system",
    "stack": "id:number input:string -> result(map)",
    "body": "Writes UTF-8 stdin text to a running retained process job that was started with `stdin_open` set to true. The result is the latest process snapshot. Closed, completed, cancelled, or unknown jobs return a result error.",
    "example": "$job \"id\" at \"command input\\n\" process_write value"
  },
  {
    "word": "process_read",
    "aliases": [],
    "group": "system",
    "stack": "id:number options:map -> result(map)",
    "body": "Reads retained stdout/stderr for a process job. Options include `stdout_offset` and `stderr_offset`; the result includes output slices, next offsets, and the same snapshot fields as `process_job`.",
    "example": "readOptions map\n$job \"id\" at $readOptions process_read value"
  },
  {
    "word": "process_env_put",
    "aliases": ["process"],
    "group": "system",
    "stack": "options:map name:string value:string -> result(map)",
    "body": "Adds or updates a child-process environment entry inside a process options map. The nested `env` map is created when missing; variable names and values are validated before use.",
    "example": "options map\n$options \"GIT_TERMINAL_PROMPT\" \"0\" process_env_put value options set"
  },
  {
    "word": "pty_start",
    "aliases": [],
    "group": "system",
    "stack": "command:string args:array options:map -> result(map)",
    "body": "Starts a command in a real pseudo-terminal when the PTY capability is enabled. Options include `cwd`, `clear_env`, `env`, `rows`, `cols`, and `output_max_bytes`.",
    "example": "args array\n$args \"repl\" push drop\noptions map\n\"rco\" $args $options pty_start value"
  },
  {
    "word": "pty_write",
    "aliases": [],
    "group": "system",
    "stack": "id:number input:string -> result(map)",
    "body": "Writes input to a running PTY session and returns the latest session snapshot.",
    "example": "$session \"id\" at \"1 2 +\\r\\n\" pty_write value"
  },
  {
    "word": "pty_read",
    "aliases": [],
    "group": "system",
    "stack": "id:number options:map -> result(map)",
    "body": "Reads retained PTY output. Option `offset` supports incremental reads; the result includes `output`, next `offset`, truncation state, size, status, and process metadata.",
    "example": "readOptions map\n$session \"id\" at $readOptions pty_read value"
  },
  {
    "word": "pty_resize",
    "aliases": [],
    "group": "system",
    "stack": "id:number cols:number rows:number -> result(map)",
    "body": "Resizes a PTY session and returns the latest snapshot.",
    "example": "$session \"id\" at 120 40 pty_resize value"
  },
  {
    "word": "pty_stop",
    "aliases": [],
    "group": "system",
    "stack": "id:number options:map -> result(map)",
    "body": "Requests termination of a PTY session and returns the latest snapshot.",
    "example": "stopOptions map\n$session \"id\" at $stopOptions pty_stop value"
  },
  {
    "word": "pty_release",
    "aliases": [],
    "group": "system",
    "stack": "id:number -> result(bool)",
    "body": "Removes a completed retained PTY session from the host registry. Running sessions return `PtyRunning`; stop or wait for completion first.",
    "example": "$session \"id\" at pty_release value"
  },
  {
    "word": "pty_list",
    "aliases": [],
    "group": "system",
    "stack": "-> array",
    "body": "Returns snapshots for retained PTY sessions in the current host registry.",
    "example": "pty_list count"
  },
  {
    "word": "pty_detail",
    "aliases": [],
    "group": "system",
    "stack": "id:number -> result(map)",
    "body": "Returns a snapshot for one retained PTY session, or a `PtyNotFound` result when the id is unknown.",
    "example": "$session \"id\" at pty_detail value"
  },
  {
    "word": "approval_create",
    "aliases": [],
    "group": "system",
    "stack": "operation:map options:map -> result(map)",
    "body": "Creates a runtime-local approval record and returns a one-time token in the create result. Options include `id`, `token`, `ttl_ms`, `expires_at_ms`, and `metadata`; unknown options fail with `ApprovalRequestError`.",
    "example": "operation map\noptions map\n$operation $options approval_create value"
  },
  {
    "word": "approval_claim",
    "aliases": [],
    "group": "system",
    "stack": "id:string token:string -> result(map)",
    "body": "Claims a pending approval exactly once. Expired, rejected, completed, already claimed, or token-mismatched approvals return structured result errors.",
    "example": "$approval \"id\" at $approval \"token\" at approval_claim value"
  },
  {
    "word": "approval_complete",
    "aliases": [],
    "group": "system",
    "stack": "id:string result:value -> result(map)",
    "body": "Marks a claimed approval completed and stores the caller-provided result value for audit. Pending approvals must be claimed before completion.",
    "example": "$approval \"id\" at $result approval_complete value"
  },
  {
    "word": "approval_reject",
    "aliases": [],
    "group": "system",
    "stack": "id:string reason:string -> result(map)",
    "body": "Marks a pending or claimed approval rejected with a reason. Final and expired approvals cannot be rejected again.",
    "example": "$approval \"id\" at \"Rejected by user\" approval_reject value"
  },
  {
    "word": "approval_detail",
    "aliases": [],
    "group": "system",
    "stack": "id:string -> result(map)",
    "body": "Returns a retained approval record without re-exposing the one-time token after creation.",
    "example": "$approval \"id\" at approval_detail value"
  },
  {
    "word": "approval_release",
    "aliases": [],
    "group": "system",
    "stack": "id:string -> result(bool)",
    "body": "Releases a retained approval record when an app no longer needs to inspect it. Approval registries are capped and also prune expired or final records under pressure.",
    "example": "$approval \"id\" at approval_release value drop"
  },
  {
    "word": "now",
    "aliases": [],
    "group": "system",
    "stack": "-> number",
    "body": "Pushes Unix epoch milliseconds.",
    "example": "now"
  },
  {
    "word": "timestamp_now",
    "aliases": ["time"],
    "group": "system",
    "stack": "-> number",
    "body": "Alias-shaped timestamp word that pushes Unix epoch milliseconds, matching `now`.",
    "example": "timestamp_now"
  },
  {
    "word": "timestamp_parse",
    "aliases": ["time", "date"],
    "group": "system",
    "stack": "rfc3339:string -> result(number)",
    "body": "Parses an RFC3339 timestamp with an offset and returns UTC Unix epoch milliseconds.",
    "example": "\"2026-06-18T13:14:15.250Z\" timestamp_parse value"
  },
  {
    "word": "timestamp_format",
    "aliases": ["time", "date"],
    "group": "system",
    "stack": "timestampMs:number -> result(string)",
    "body": "Formats UTC Unix epoch milliseconds as RFC3339 with millisecond precision.",
    "example": "$timestamp timestamp_format value"
  },
  {
    "word": "timestamp_format_pattern",
    "aliases": ["time", "date"],
    "group": "system",
    "stack": "timestampMs:number pattern:string -> result(string)",
    "body": "Formats UTC Unix epoch milliseconds with a Chrono/strftime-style pattern such as `%Y-%m-%d`.",
    "example": "$timestamp \"%Y-%m-%d %H:%M:%S\" timestamp_format_pattern value"
  },
  {
    "word": "timestamp_parts",
    "aliases": ["time", "date"],
    "group": "system",
    "stack": "timestampMs:number -> result(map)",
    "body": "Breaks UTC Unix epoch milliseconds into `year`, `month`, `day`, `hour`, `minute`, `second`, `millisecond`, `weekday`, and ordinal fields.",
    "example": "$timestamp timestamp_parts value"
  },
  {
    "word": "timestamp_from_parts",
    "aliases": ["time", "date"],
    "group": "system",
    "stack": "parts:map -> result(number)",
    "body": "Builds UTC Unix epoch milliseconds from a parts map. `year`, `month`, and `day` are required; time fields default to zero.",
    "example": "$parts timestamp_from_parts value"
  },
  {
    "word": "timestamp_add",
    "aliases": ["time", "date"],
    "group": "system",
    "stack": "timestampMs:number durationMs:number -> result(number)",
    "body": "Adds a duration in milliseconds to UTC Unix epoch milliseconds, checking overflow and supported timestamp range.",
    "example": "$timestamp 2 duration_hours value timestamp_add value"
  },
  {
    "word": "timestamp_diff",
    "aliases": ["time", "date"],
    "group": "system",
    "stack": "startMs:number endMs:number -> result(number)",
    "body": "Returns `endMs - startMs` in milliseconds.",
    "example": "$start $end timestamp_diff value"
  },
  {
    "word": "date_from_timestamp",
    "aliases": ["date", "time"],
    "group": "system",
    "stack": "timestampMs:number -> result(map)",
    "body": "Converts UTC Unix epoch milliseconds into a date map with `year`, `month`, `day`, `weekday`, and ordinal fields.",
    "example": "$timestamp date_from_timestamp value"
  },
  {
    "word": "date_to_timestamp",
    "aliases": ["date", "time"],
    "group": "system",
    "stack": "date:map -> result(number)",
    "body": "Converts a date map into UTC Unix epoch milliseconds at midnight.",
    "example": "$date date_to_timestamp value"
  },
  {
    "word": "date_parse",
    "aliases": ["date"],
    "group": "system",
    "stack": "date:string -> result(map)",
    "body": "Parses an ISO `YYYY-MM-DD` date string into a date map.",
    "example": "\"2026-02-28\" date_parse value"
  },
  {
    "word": "date_format",
    "aliases": ["date"],
    "group": "system",
    "stack": "date:map pattern:string -> result(string)",
    "body": "Formats a date map with a Chrono/strftime-style pattern such as `%Y/%m/%d`.",
    "example": "$date \"%Y/%m/%d\" date_format value"
  },
  {
    "word": "date_add_days",
    "aliases": ["date"],
    "group": "system",
    "stack": "date:map days:number -> result(map)",
    "body": "Adds signed days to a date map.",
    "example": "$date 1 date_add_days value"
  },
  {
    "word": "date_diff_days",
    "aliases": ["date"],
    "group": "system",
    "stack": "start:map end:map -> result(number)",
    "body": "Returns `end - start` in whole calendar days.",
    "example": "$startDate $endDate date_diff_days value"
  },
  {
    "word": "duration_millis",
    "aliases": ["time"],
    "group": "system",
    "stack": "millis:number -> result(number)",
    "body": "Builds a duration in milliseconds.",
    "example": "250 duration_millis value"
  },
  {
    "word": "duration_seconds",
    "aliases": ["time"],
    "group": "system",
    "stack": "seconds:number -> result(number)",
    "body": "Builds a duration in milliseconds from seconds.",
    "example": "30 duration_seconds value"
  },
  {
    "word": "duration_minutes",
    "aliases": ["time"],
    "group": "system",
    "stack": "minutes:number -> result(number)",
    "body": "Builds a duration in milliseconds from minutes.",
    "example": "15 duration_minutes value"
  },
  {
    "word": "duration_hours",
    "aliases": ["time"],
    "group": "system",
    "stack": "hours:number -> result(number)",
    "body": "Builds a duration in milliseconds from hours.",
    "example": "2 duration_hours value"
  },
  {
    "word": "duration_days",
    "aliases": ["time"],
    "group": "system",
    "stack": "days:number -> result(number)",
    "body": "Builds a duration in milliseconds from days.",
    "example": "7 duration_days value"
  },
  {
    "word": "duration_weeks",
    "aliases": ["time"],
    "group": "system",
    "stack": "weeks:number -> result(number)",
    "body": "Builds a duration in milliseconds from weeks.",
    "example": "1 duration_weeks value"
  },
  {
    "word": "duration_parts",
    "aliases": ["time"],
    "group": "system",
    "stack": "durationMs:number -> result(map)",
    "body": "Breaks a duration into `days`, `hours`, `minutes`, `seconds`, `milliseconds`, `total_ms`, and `negative` fields.",
    "example": "$duration duration_parts value"
  },
  {
    "word": "sleep",
    "aliases": [],
    "group": "system",
    "stack": "millis:number ->",
    "body": "Sleeps the current VM thread for a non-negative number of milliseconds.",
    "example": "100 sleep"
  },
  {
    "word": "random",
    "aliases": [],
    "group": "system",
    "stack": "max:number -> number",
    "body": "Returns a non-cryptographic random number from 0 up to max.",
    "example": "100 random"
  },
  {
    "word": "exit",
    "aliases": [],
    "group": "system",
    "stack": "code:number -> exits",
    "body": "Requests process exit with the given status code.",
    "example": "0 exit"
  },
  {
    "word": "fs_read_text",
    "aliases": [],
    "group": "system",
    "stack": "path:string -> result(string)",
    "body": "Reads a UTF-8 text file through the filesystem capability.",
    "example": "\"README.md\" fs_read_text value"
  },
  {
    "word": "fs_write_text",
    "aliases": [],
    "group": "system",
    "stack": "path:string contents:string -> result(path)",
    "body": "Writes a UTF-8 text file through the filesystem capability.",
    "example": "\"out.txt\" \"hello\" fs_write_text value"
  },
  {
    "word": "fs_exists?",
    "aliases": [],
    "group": "system",
    "stack": "path:string -> bool",
    "body": "Checks file or directory existence through the filesystem capability.",
    "example": "\"README.md\" fs_exists?"
  },
  {
    "word": "fs_list",
    "aliases": [],
    "group": "system",
    "stack": "path:string -> result(array)",
    "body": "Lists directory entries as path strings through the filesystem capability.",
    "example": "\".\" fs_list value"
  },
  {
    "word": "fs_create_dir",
    "aliases": [],
    "group": "system",
    "stack": "path:string -> result(path)",
    "body": "Creates a directory and parents if needed through the filesystem capability.",
    "example": "\"tmp/cache\" fs_create_dir value"
  },
  {
    "word": "fs_delete",
    "aliases": [],
    "group": "system",
    "stack": "path:string -> result(path)",
    "body": "Deletes a file, symlink, or empty directory through the filesystem capability. Denied when filesystem writes are disabled or when the requested path is the configured filesystem root.",
    "example": "\"tmp/cache/file.txt\" fs_delete value"
  },
  {
    "word": "workspace_resolve",
    "aliases": [],
    "group": "system",
    "stack": "path:string options:map -> result(map)",
    "body": "Resolves a workspace path through the filesystem root and returns a structured map with requested path, resolved path, relative path, root containment, and existence.",
    "example": "options map\n\".\" $options workspace_resolve value"
  },
  {
    "word": "workspace_contains?",
    "aliases": [],
    "group": "system",
    "stack": "root:string path:string -> bool",
    "body": "Returns true when both paths resolve through the filesystem capability and the candidate is physically inside the resolved root. The predicate canonicalizes the existing root and the candidate's nearest existing ancestor, accepts links whose physical targets remain in-root, and returns false for unresolvable roots or physical escapes. This is a point-in-time check, not an atomic access guarantee.",
    "example": "\".\" \"src/main.rco\" workspace_contains?"
  },
  {
    "word": "workspace_metadata",
    "aliases": [],
    "group": "system",
    "stack": "path:string -> result(map)",
    "body": "Returns structured metadata for a workspace file, directory, symlink, or other entry.",
    "example": "\"README.md\" workspace_metadata value"
  },
  {
    "word": "workspace_list",
    "aliases": [],
    "group": "system",
    "stack": "path:string options:map -> result(array)",
    "body": "Lists workspace entries as metadata maps. Options include `recursive`, `include_files`, `include_dirs`, `max_entries`, and Boolean `truncate_on_limit`. By default, exceeding `max_entries` returns `IoError`; when truncation is enabled, recursive traversal stops globally and returns the capped Array.",
    "example": "options map\n\".\" $options workspace_list value"
  },
  {
    "word": "workspace_read_text",
    "aliases": [],
    "group": "system",
    "stack": "path:string options:map -> result(string)",
    "body": "Reads UTF-8 workspace text with a bounded `max_bytes` option. The default cap is 1 MiB.",
    "example": "options map\n\"README.md\" $options workspace_read_text value"
  },
  {
    "word": "workspace_read_text_snapshot",
    "aliases": [],
    "group": "system",
    "stack": "path:string options:map -> result(map)",
    "body": "Reads one bounded UTF-8 byte buffer and returns `text`, its `sha256` integrity string, and byte `len` together with workspace path fields. Option `max_bytes` defaults to 1 MiB.",
    "example": "options map\n\"README.md\" $options workspace_read_text_snapshot value snapshot var\n$snapshot \"sha256\" at println"
  },
  {
    "word": "workspace_write_text",
    "aliases": [],
    "group": "system",
    "stack": "path:string contents:string options:map -> result(map)",
    "body": "Writes UTF-8 text through the workspace bounds when filesystem writes are enabled. Options are `overwrite`, `create_parent_dirs`, and `expected_sha256`; overwrite defaults to false and expected_sha256 requires overwrite true. A non-nil `expected_sha256` uses the exact format `sha256:` followed by exactly 64 lowercase hexadecimal characters. Invalid types or syntax, and a non-nil precondition with overwrite false, return WorkspaceRequestError. Success extends workspace metadata with `atomic`, `bytes_written`, `sha256_before`, and `sha256_after`. With overwrite false, creation exclusively claims an absent path and returns AlreadyExists to a losing creator, but the winning file may be visible before its contents finish writing. With overwrite true, Ricochet synchronizes a same-directory staging file and atomically replaces the path; the containing directory is not synchronized, so this is not a complete storage-durability guarantee. `expected_sha256` coordinates competing calls that share one host registry and returns PreconditionFailed when the destination no longer matches; external mutations remain outside that guarantee. Successful replacement of an existing file preserves only Rust's `std::fs::Permissions`; owner, ACLs, xattrs, alternate data streams, timestamps, and hard-link identity are excluded. Failures after staging begins and before replacement retain the staging file and report its path. PostCommitMetadataError means replacement committed but metadata inspection failed, so do not retry blindly.",
    "example": "options map\n\"out.txt\" \"hello\" $options workspace_write_text value"
  },
  {
    "word": "workspace_mkdir",
    "aliases": [],
    "group": "system",
    "stack": "path:string options:map -> result(map)",
    "body": "Creates a workspace directory when filesystem writes are enabled. Option `recursive` defaults to true.",
    "example": "options map\n\"tmp/cache\" $options workspace_mkdir value"
  },
  {
    "word": "workspace_delete",
    "aliases": [],
    "group": "system",
    "stack": "path:string options:map -> result(map)",
    "body": "Deletes a workspace file, symlink, empty directory, or recursive directory when filesystem writes are enabled. Options include `recursive` and `missing_ok`, both defaulting to false. The configured filesystem root itself is never deleted.",
    "example": "options map\n$options \"recursive\" true put drop\n\"tmp/cache\" $options workspace_delete value"
  },
  {
    "word": "workspace_copy",
    "aliases": [],
    "group": "system",
    "stack": "source:string destination:string options:map -> result(map)",
    "body": "Copies a file inside workspace bounds. Options include `overwrite` and `create_parent_dirs`; overwrite defaults to false. No-overwrite behavior remains best-effort under concurrent destination creation.",
    "example": "options map\n\"README.md\" \"tmp/README.md\" $options workspace_copy value"
  },
  {
    "word": "workspace_move",
    "aliases": [],
    "group": "system",
    "stack": "source:string destination:string options:map -> result(map)",
    "body": "Renames a file or directory inside workspace bounds. Existing destinations are rejected. No-overwrite behavior remains best-effort under concurrent destination creation.",
    "example": "options map\n\"tmp/a.txt\" \"tmp/b.txt\" $options workspace_move value"
  },
  {
    "word": "http_request_new",
    "aliases": ["HTTP"],
    "group": "system",
    "stack": "method:string url:string -> result(map)",
    "body": "Creates a structured HTTP request map with validated method and URL fields. Add headers, JSON bodies, and timeouts before passing it to `http_request` or `http_request_task`.",
    "example": "\"POST\" \"https://api.example/v1\" http_request_new value"
  },
  {
    "word": "http_header_put",
    "aliases": ["HTTP", "headers"],
    "group": "system",
    "stack": "request:map name:string value:string -> result(map)",
    "body": "Adds or updates a validated HTTP header inside a request map, creating the nested `headers` map when needed.",
    "example": "$request \"Accept\" \"application/json\" http_header_put value request set"
  },
  {
    "word": "http_bearer_auth",
    "aliases": ["HTTP", "secrets"],
    "group": "system",
    "stack": "request:map token:string -> result(map)",
    "body": "Adds an `Authorization: Bearer ...` header to a request map. This is intended to be paired with `secret_resolve` for env-backed API keys.",
    "example": "$request $token http_bearer_auth value request set"
  },
  {
    "word": "http_json_body",
    "aliases": ["HTTP", "JSON"],
    "group": "system",
    "stack": "request:map body:any -> result(map)",
    "body": "Stores a Ricochet value as the JSON body for a structured HTTP request and clears any string body already present.",
    "example": "$request $payload http_json_body value request set"
  },
  {
    "word": "http_timeout",
    "aliases": ["HTTP"],
    "group": "system",
    "stack": "request:map millis:number -> result(map)",
    "body": "Sets a bounded request timeout in milliseconds on a structured HTTP request map.",
    "example": "$request 30000 http_timeout value request set"
  },
  {
    "word": "http_get",
    "aliases": [],
    "group": "system",
    "stack": "url:string -> result(map)",
    "body": "Runs an HTTP GET and returns a result map with status, body, and headers.",
    "example": "\"https://example.com\" http_get value"
  },
  {
    "word": "http_get_task",
    "aliases": [],
    "group": "system",
    "stack": "url:string -> task",
    "body": "Starts an HTTP GET on a task worker. Await the task to receive the same result map returned by `http_get`.",
    "example": "\"https://example.com\" http_get_task await value"
  },
  {
    "word": "http_request",
    "aliases": ["HTTP"],
    "group": "system",
    "stack": "request:map -> result(map)",
    "body": "Runs an HTTP request from a map with `url`, optional `method`, optional `headers`, and optional `json` or string `body`. Request maps may also include `timeout_ms`, `max_response_bytes`, `allowed_hosts`, `allowed_schemes`, and `follow_redirects=false`; redirects remain disabled.",
    "example": "$request \"timeout_ms\" 30000 put drop\n$request http_request value"
  },
  {
    "word": "http_post_json",
    "aliases": ["HTTP"],
    "group": "system",
    "stack": "url:string body:any -> result(map)",
    "body": "Posts a JSON-encoded Ricochet value.",
    "example": "\"https://api.example\" $payload http_post_json"
  },
  {
    "word": "http_post_json_task",
    "aliases": ["HTTP", "async"],
    "group": "system",
    "stack": "url:string body:any -> task",
    "body": "Starts a JSON HTTP POST on a task worker. Await the task to receive the same result map returned by `http_post_json`.",
    "example": "\"https://api.example\" $payload http_post_json_task await value"
  },
  {
    "word": "http_request_task",
    "aliases": ["HTTP", "headers", "async"],
    "group": "system",
    "stack": "request:map -> task",
    "body": "Starts a mapped HTTP request on a task worker. Await the task to receive the same result map returned by `http_request`, including request-level timeout, byte-cap, scheme, and host policy checks.",
    "example": "$request http_request_task await value"
  },
  {
    "word": "http_stream_start",
    "aliases": ["HTTP", "stream"],
    "group": "system",
    "stack": "request:map -> result(map)",
    "body": "Starts a mapped HTTP request as a retained streaming job. The returned snapshot includes `id`, `status`, `running`, `success`, `status_code`, `headers`, `body_len`, `body_truncated`, and cancellation state. Use `http_stream_read` with offsets to consume retained response bytes while the request is still running. Retained streams are capped; release completed streams with `http_stream_release`.",
    "example": "$request http_stream_start value stream var"
  },
  {
    "word": "http_streams",
    "aliases": ["HTTP", "stream"],
    "group": "system",
    "stack": "-> array",
    "body": "Returns snapshots for retained HTTP stream jobs in the current VM host.",
    "example": "http_streams count"
  },
  {
    "word": "http_stream",
    "aliases": ["HTTP", "stream"],
    "group": "system",
    "stack": "id:number -> result(map)",
    "body": "Returns the latest snapshot for a retained HTTP stream job.",
    "example": "$stream \"id\" at http_stream value"
  },
  {
    "word": "http_stream_read",
    "aliases": ["HTTP", "stream"],
    "group": "system",
    "stack": "id:number options:map -> result(map)",
    "body": "Reads retained HTTP stream body text from `offset`, optionally bounded by `max_bytes`, and returns the same snapshot fields plus `body`, `from_offset`, `next_offset`, backward-compatible `offset` as the next offset, `bytes_len`, and `done`. Missing or nil `offset` starts at 0; missing or nil `max_bytes` reads all currently retained bytes from that offset. `done` is true only when the stream is no longer running and the read consumed all retained bytes.",
    "example": "options map\n$options \"max_bytes\" 4096 put drop\n$stream \"id\" at $options http_stream_read value"
  },
  {
    "word": "http_stream_cancel",
    "aliases": ["HTTP", "stream"],
    "group": "system",
    "stack": "id:number -> result(map)",
    "body": "Requests cancellation for a retained HTTP stream job and returns its latest snapshot.",
    "example": "$stream \"id\" at http_stream_cancel value"
  },
  {
    "word": "http_stream_release",
    "aliases": ["HTTP", "stream"],
    "group": "system",
    "stack": "id:number -> result(bool)",
    "body": "Removes a completed retained HTTP stream job from the host registry. Running streams return `HttpStreamRunning`; cancel or wait for completion first.",
    "example": "$stream \"id\" at http_stream_release value"
  },
  {
    "word": "upload_streams",
    "aliases": ["MVC", "upload", "stream"],
    "group": "system",
    "stack": "-> array",
    "body": "Returns retained MVC upload stream snapshots for the current request VM. Each snapshot includes `id`, `field`, `filename`, `content_type`, `size_known`, and `size`. Release streams when finished with `upload_release`.",
    "example": "upload_streams count"
  },
  {
    "word": "upload_stream",
    "aliases": ["MVC", "upload", "stream"],
    "group": "system",
    "stack": "id:number -> result(map)",
    "body": "Returns metadata for a retained MVC upload stream by id. Unknown ids return `UnknownUploadStream` as a Result error.",
    "example": "$file \"stream_id\" at upload_stream value"
  },
  {
    "word": "upload_read",
    "aliases": ["MVC", "upload", "stream"],
    "group": "system",
    "stack": "id:number options:map -> result(map)",
    "body": "Reads a bounded chunk from a retained MVC upload stream. Options may include `offset` and `max_bytes`; the result includes upload metadata plus `offset`, `next_offset`, `eof`, `bytes_len`, `data_base64`, and UTF-8 `text` when available.",
    "example": "options map\noptions \"max_bytes\" 4096 put drop\n$file \"stream_id\" at $options upload_read value"
  },
  {
    "word": "upload_release",
    "aliases": ["MVC", "upload", "stream"],
    "group": "system",
    "stack": "id:number -> result(bool)",
    "body": "Releases a retained MVC upload stream and removes its temporary file. Returns `ok(true)` when a stream was released and `ok(false)` for an already-missing id.",
    "example": "$file \"stream_id\" at upload_release value"
  },
  {
    "word": "tcp_listen",
    "aliases": ["socket", "TCP", "server"],
    "group": "system",
    "stack": "host:string port:number options:map -> result(map)",
    "body": "Binds a retained TCP listener when socket capability is enabled. Port `0` asks the OS for an available port. Options include `nodelay`; `--socket-allow-host` can restrict bind hosts. The returned snapshot includes `id`, actual `port`, `status`, local address, and accepted-connection count.",
    "example": "options map\n\"127.0.0.1\" 0 $options tcp_listen value listener var"
  },
  {
    "word": "tcp_listeners",
    "aliases": ["socket", "TCP", "server"],
    "group": "system",
    "stack": "-> array",
    "body": "Returns snapshots for retained TCP listeners in the current VM host.",
    "example": "tcp_listeners count"
  },
  {
    "word": "tcp_listener",
    "aliases": ["socket", "TCP", "server"],
    "group": "system",
    "stack": "id:number -> result(map)",
    "body": "Returns a snapshot for one retained TCP listener, or an `UnknownTcpListener` result when the id is unknown.",
    "example": "$listener \"id\" at tcp_listener value"
  },
  {
    "word": "tcp_accept",
    "aliases": ["socket", "TCP", "server"],
    "group": "system",
    "stack": "listener_id:number options:map -> result(map)",
    "body": "Accepts one inbound connection from a retained TCP listener with bounded `timeout_ms`. The accepted connection is retained as an ordinary TCP socket and can be used with `tcp_read`, `tcp_write`, `tcp_close`, and `tcp_release`.",
    "example": "acceptOptions map\n$listener \"id\" at $acceptOptions tcp_accept value socket var"
  },
  {
    "word": "tcp_listener_close",
    "aliases": ["socket", "TCP", "server"],
    "group": "system",
    "stack": "id:number -> result(map)",
    "body": "Closes a retained TCP listener and returns the latest listener snapshot. Accepted TCP sockets remain independent retained connections.",
    "example": "$listener \"id\" at tcp_listener_close value"
  },
  {
    "word": "tcp_listener_release",
    "aliases": ["socket", "TCP", "server"],
    "group": "system",
    "stack": "id:number -> result(bool)",
    "body": "Removes a closed retained TCP listener from the host registry. Listening sockets return `TcpListenerOpen`; close them first.",
    "example": "$listener \"id\" at tcp_listener_release value"
  },
  {
    "word": "tcp_connect",
    "aliases": ["socket", "TCP"],
    "group": "system",
    "stack": "host:string port:number options:map -> result(map)",
    "body": "Opens an outbound TCP connection when socket capability is enabled. Options include bounded `timeout_ms` and `nodelay`; `--socket-allow-host` can restrict destination hosts. The returned snapshot includes `id`, `status`, address fields, and byte counters.",
    "example": "options map\n\"127.0.0.1\" 9000 $options tcp_connect value socket var"
  },
  {
    "word": "tcp_connections",
    "aliases": ["socket", "TCP"],
    "group": "system",
    "stack": "-> array",
    "body": "Returns snapshots for retained TCP socket connections in the current VM host.",
    "example": "tcp_connections count"
  },
  {
    "word": "tcp_connection",
    "aliases": ["socket", "TCP"],
    "group": "system",
    "stack": "id:number -> result(map)",
    "body": "Returns a snapshot for one retained TCP socket, or an `UnknownTcpSocket` result when the id is unknown.",
    "example": "$socket \"id\" at tcp_connection value"
  },
  {
    "word": "tcp_write",
    "aliases": ["socket", "TCP"],
    "group": "system",
    "stack": "id:number data:string -> result(map)",
    "body": "Writes UTF-8 text bytes to a connected TCP socket and returns the latest snapshot with updated byte counters.",
    "example": "$socket \"id\" at \"ping\" tcp_write value"
  },
  {
    "word": "tcp_read",
    "aliases": ["socket", "TCP"],
    "group": "system",
    "stack": "id:number options:map -> result(map)",
    "body": "Reads up to `max_bytes` from a connected TCP socket with a bounded `timeout_ms`. The result includes snapshot fields plus `data` and `bytes`; timeout without data returns an empty chunk.",
    "example": "readOptions map\n$socket \"id\" at $readOptions tcp_read value"
  },
  {
    "word": "tcp_close",
    "aliases": ["socket", "TCP"],
    "group": "system",
    "stack": "id:number -> result(map)",
    "body": "Closes a retained TCP socket and returns the latest snapshot.",
    "example": "$socket \"id\" at tcp_close value"
  },
  {
    "word": "tcp_release",
    "aliases": ["socket", "TCP"],
    "group": "system",
    "stack": "id:number -> result(bool)",
    "body": "Removes a closed retained TCP socket from the host registry. Connected sockets return `TcpSocketOpen`; close them first.",
    "example": "$socket \"id\" at tcp_release value"
  },
  {
    "word": "ws_listen",
    "aliases": ["socket", "WebSocket", "server"],
    "group": "system",
    "stack": "host:string port:number options:map -> result(map)",
    "body": "Binds a retained WebSocket listener when socket capability is enabled. Port `0` asks the OS for an available port; `--socket-allow-host` can restrict bind hosts. Accepted sockets are retained as ordinary WebSocket connections.",
    "example": "options map\n\"127.0.0.1\" 0 $options ws_listen value listener var"
  },
  {
    "word": "ws_listeners",
    "aliases": ["socket", "WebSocket", "server"],
    "group": "system",
    "stack": "-> array",
    "body": "Returns snapshots for retained WebSocket listeners in the current VM host.",
    "example": "ws_listeners count"
  },
  {
    "word": "ws_listener",
    "aliases": ["socket", "WebSocket", "server"],
    "group": "system",
    "stack": "id:number -> result(map)",
    "body": "Returns a snapshot for one retained WebSocket listener, or an `UnknownWebSocketListener` result when the id is unknown.",
    "example": "$listener \"id\" at ws_listener value"
  },
  {
    "word": "ws_accept",
    "aliases": ["socket", "WebSocket", "server"],
    "group": "system",
    "stack": "listener_id:number options:map -> result(map)",
    "body": "Accepts one inbound WebSocket handshake from a retained listener with bounded `timeout_ms`. The accepted socket is retained as an ordinary WebSocket and can be used with `ws_read`, `ws_send`, `ws_close`, and `ws_release`.",
    "example": "acceptOptions map\n$listener \"id\" at $acceptOptions ws_accept value socket var"
  },
  {
    "word": "ws_listener_close",
    "aliases": ["socket", "WebSocket", "server"],
    "group": "system",
    "stack": "id:number -> result(map)",
    "body": "Closes a retained WebSocket listener and returns the latest listener snapshot. Accepted WebSockets remain independent retained connections.",
    "example": "$listener \"id\" at ws_listener_close value"
  },
  {
    "word": "ws_listener_release",
    "aliases": ["socket", "WebSocket", "server"],
    "group": "system",
    "stack": "id:number -> result(bool)",
    "body": "Removes a closed retained WebSocket listener from the host registry. Listening sockets return `WebSocketListenerOpen`; close them first.",
    "example": "$listener \"id\" at ws_listener_release value"
  },
  {
    "word": "ws_connect",
    "aliases": ["socket", "WebSocket"],
    "group": "system",
    "stack": "url:string options:map -> result(map)",
    "body": "Opens an outbound `ws://` or `wss://` WebSocket when socket capability is enabled. Options include bounded `timeout_ms`; `--socket-allow-host` can restrict destination hosts. The returned snapshot includes `id`, `status`, response metadata, and message counters.",
    "example": "options map\n\"ws://127.0.0.1:9001/echo\" $options ws_connect value socket var"
  },
  {
    "word": "ws_connections",
    "aliases": ["socket", "WebSocket"],
    "group": "system",
    "stack": "-> array",
    "body": "Returns snapshots for retained WebSocket connections in the current VM host.",
    "example": "ws_connections count"
  },
  {
    "word": "ws_connection",
    "aliases": ["socket", "WebSocket"],
    "group": "system",
    "stack": "id:number -> result(map)",
    "body": "Returns a snapshot for one retained WebSocket, or an `UnknownWebSocket` result when the id is unknown.",
    "example": "$socket \"id\" at ws_connection value"
  },
  {
    "word": "ws_send",
    "aliases": ["socket", "WebSocket"],
    "group": "system",
    "stack": "id:number message:string -> result(map)",
    "body": "Sends a text message on a connected WebSocket and returns the latest snapshot with updated message counters.",
    "example": "$socket \"id\" at \"hello\" ws_send value"
  },
  {
    "word": "ws_read",
    "aliases": ["socket", "WebSocket"],
    "group": "system",
    "stack": "id:number options:map -> result(map)",
    "body": "Reads one WebSocket message with a bounded `timeout_ms`. The result includes snapshot fields plus `message_type`, `message`, and `bytes`; timeout without data returns `message_type` `none`.",
    "example": "readOptions map\n$socket \"id\" at $readOptions ws_read value"
  },
  {
    "word": "ws_close",
    "aliases": ["socket", "WebSocket"],
    "group": "system",
    "stack": "id:number -> result(map)",
    "body": "Closes a retained WebSocket and returns the latest snapshot.",
    "example": "$socket \"id\" at ws_close value"
  },
  {
    "word": "ws_release",
    "aliases": ["socket", "WebSocket"],
    "group": "system",
    "stack": "id:number -> result(bool)",
    "body": "Removes a closed retained WebSocket from the host registry. Connected sockets return `WebSocketOpen`; close them first.",
    "example": "$socket \"id\" at ws_release value"
  },
  {
    "word": "tui_enter",
    "aliases": ["terminal", "UI"],
    "group": "system",
    "stack": "-> result(nil)",
    "body": "Enters the alternate screen, enables raw mode, hides the cursor, clears the terminal, and moves to the top-left cell.",
    "example": "tui_enter value drop"
  },
  {
    "word": "tui_leave",
    "aliases": ["tui"],
    "group": "system",
    "stack": "-> result(nil)",
    "body": "Shows the cursor, leaves the alternate screen, and disables raw mode.",
    "example": "tui_leave value drop"
  },
  {
    "word": "tui_clear",
    "aliases": ["tui"],
    "group": "system",
    "stack": "-> result(nil)",
    "body": "Clears the terminal and moves to the top-left cell.",
    "example": "tui_clear value drop"
  },
  {
    "word": "tui_move_to",
    "aliases": ["tui"],
    "group": "system",
    "stack": "column:number row:number -> result(nil)",
    "body": "Queues a cursor move to a zero-based terminal column and row.",
    "example": "0 2 tui_move_to value drop"
  },
  {
    "word": "tui_write",
    "aliases": ["tui"],
    "group": "system",
    "stack": "text:string -> result(nil)",
    "body": "Queues text for terminal output. Use `tui_flush` to force queued output to the host stream.",
    "example": "\"Hello TUI\" tui_write value drop"
  },
  {
    "word": "tui_flush",
    "aliases": ["tui"],
    "group": "system",
    "stack": "-> result(nil)",
    "body": "Flushes queued terminal output.",
    "example": "tui_flush value drop"
  },
  {
    "word": "tui_size",
    "aliases": ["tui"],
    "group": "system",
    "stack": "-> result(map)",
    "body": "Returns terminal size as a result map with `columns` and `rows`.",
    "example": "tui_size value"
  },
  {
    "word": "tui_poll_key",
    "aliases": ["tui"],
    "group": "system",
    "stack": "timeoutMs:number -> result(map|nil)",
    "body": "Polls for a key with a non-negative timeout in milliseconds and returns nil when no key is ready.",
    "example": "0 tui_poll_key value"
  },
  {
    "word": "tui_read_key",
    "aliases": ["tui"],
    "group": "system",
    "stack": "-> result(map)",
    "body": "Blocks until a key is read and returns a map with `type`, `code`, `char`, and `modifiers`.",
    "example": "tui_read_key value"
  },
  {
    "word": "webview_text",
    "aliases": ["desktop", "UI"],
    "group": "system",
    "stack": "text:string -> html:string",
    "body": "Escapes plain text for insertion into a webview HTML fragment.",
    "example": "\"Ada <Lovelace>\" webview_text"
  },
  {
    "word": "webview_heading",
    "aliases": ["webview"],
    "group": "system",
    "stack": "text:string level:number -> html:string",
    "body": "Builds an escaped `<h1>` through `<h6>` webview heading fragment.",
    "example": "\"Counter\" 1 webview_heading"
  },
  {
    "word": "webview_button",
    "aliases": ["webview"],
    "group": "system",
    "stack": "label:string action:string -> html:string",
    "body": "Builds an escaped button with a `data-rco-action` attribute for GUI action dispatch.",
    "example": "\"Increment\" \"increment\" webview_button"
  },
  {
    "word": "web_command",
    "aliases": ["webview", "menu"],
    "group": "system",
    "stack": "action:string label:string shortcut:string -> map",
    "body": "Builds a native menu command descriptor. Command action IDs dispatch through matching `webview_action` callbacks.",
    "example": "\"save\" \"Save\" \"Ctrl+S\" web_command"
  },
  {
    "word": "web_command_button",
    "aliases": ["webview"],
    "group": "system",
    "stack": "label:string action:string -> html:string",
    "body": "Builds a styled app-kit button with a `data-rco-action` attribute.",
    "example": "\"Save\" \"save\" web_command_button"
  },
  {
    "word": "webview_action",
    "aliases": ["webview"],
    "group": "system",
    "stack": "label:string action:string callback:string -> map",
    "body": "Builds a GUI action descriptor map with a callback word name.",
    "example": "\"Increment\" \"increment\" \"increment_counter\" webview_action"
  },
  {
    "word": "webview_input",
    "aliases": ["webview"],
    "group": "system",
    "stack": "name:string value:string -> html:string",
    "body": "Builds an escaped text input fragment.",
    "example": "\"name\" \"Ada\" webview_input"
  },
  {
    "word": "webview_link",
    "aliases": ["webview"],
    "group": "system",
    "stack": "label:string href:string -> html:string",
    "body": "Builds an escaped anchor fragment.",
    "example": "\"Docs\" \"https://try.ricochet.today\" webview_link"
  },
  {
    "word": "webview_container",
    "aliases": ["webview"],
    "group": "system",
    "stack": "bodyHtml:string -> html:string",
    "body": "Wraps already-built webview HTML in a container element.",
    "example": "$body webview_container"
  },
  {
    "word": "web_toolbar",
    "aliases": ["webview", "app-kit"],
    "group": "system",
    "stack": "bodyHtml:string -> html:string",
    "body": "Wraps controls in a desktop app toolbar fragment.",
    "example": "$buttons web_toolbar"
  },
  {
    "word": "web_sidebar",
    "aliases": ["webview", "app-kit"],
    "group": "system",
    "stack": "bodyHtml:string -> html:string",
    "body": "Wraps navigation or supporting content in a sidebar fragment.",
    "example": "$nav web_sidebar"
  },
  {
    "word": "web_tabs",
    "aliases": ["webview", "app-kit"],
    "group": "system",
    "stack": "tabs:array -> html:string",
    "body": "Builds a tabbed fragment from tab maps with `label`, `body`, and optional `active` fields.",
    "example": "$tabs web_tabs"
  },
  {
    "word": "web_split_pane",
    "aliases": ["webview", "app-kit"],
    "group": "system",
    "stack": "leftHtml:string rightHtml:string -> html:string",
    "body": "Builds a two-pane desktop layout fragment.",
    "example": "$sidebar $content web_split_pane"
  },
  {
    "word": "web_table",
    "aliases": ["webview", "app-kit"],
    "group": "system",
    "stack": "rows:array -> html:string",
    "body": "Builds a simple table from an array or list of row maps.",
    "example": "$rows web_table"
  },
  {
    "word": "web_form_row",
    "aliases": ["webview", "app-kit"],
    "group": "system",
    "stack": "label:string controlHtml:string -> html:string",
    "body": "Builds a labeled form row around an existing control fragment.",
    "example": "\"Name\" $input web_form_row"
  },
  {
    "word": "web_status_bar",
    "aliases": ["webview", "app-kit"],
    "group": "system",
    "stack": "text:string -> html:string",
    "body": "Builds a compact status bar fragment.",
    "example": "\"Ready\" web_status_bar"
  },
  {
    "word": "web_menu",
    "aliases": ["webview", "menu"],
    "group": "system",
    "stack": "label:string items:array -> map",
    "body": "Builds a native menu descriptor from command or separator item maps.",
    "example": "\"File\" $commands web_menu"
  },
  {
    "word": "web_menu_bar",
    "aliases": ["webview", "menu"],
    "group": "system",
    "stack": "menus:array -> map",
    "body": "Builds a native menu bar descriptor for `webview_window_app`.",
    "example": "$menus web_menu_bar"
  },
  {
    "word": "webview_window",
    "aliases": ["webview_document"],
    "group": "system",
    "stack": "title:string bodyHtml:string -> result(map)",
    "body": "Builds a webview document map with `type`, `title`, `body`, full `html`, default `width`/`height`, empty `state`, and empty `actions` fields for `rco gui` and `rco package --gui` hosts.",
    "example": "\"Counter\" $body webview_window value"
  },
  {
    "word": "webview_window_state",
    "aliases": ["webview_document"],
    "group": "system",
    "stack": "title:string bodyHtml:string state:map actions:array -> result(map)",
    "body": "Builds a webview document map with explicit `state` and `actions`; action callbacks receive `(state event -> document)`.",
    "example": "\"Counter\" $body $state $actions webview_window_state value"
  },
  {
    "word": "webview_window_app",
    "aliases": ["webview_document"],
    "group": "system",
    "stack": "title:string bodyHtml:string state:map actions:array menuBar:map -> result(map)",
    "body": "Builds a stateful webview app document with native OS menu metadata.",
    "example": "\"Counter\" $body $state $actions $menuBar webview_window_app value"
  },
  {
    "word": "webview_open_file",
    "aliases": ["webview", "shell"],
    "group": "system",
    "stack": "-> result(string|nil)",
    "body": "Opens a native file picker and returns the selected path, or nil when canceled.",
    "example": "webview_open_file"
  },
  {
    "word": "webview_save_file",
    "aliases": ["webview", "shell"],
    "group": "system",
    "stack": "-> result(string|nil)",
    "body": "Opens a native save-file picker and returns the selected path, or nil when canceled.",
    "example": "webview_save_file"
  },
  {
    "word": "webview_choose_folder",
    "aliases": ["webview", "shell"],
    "group": "system",
    "stack": "-> result(string|nil)",
    "body": "Opens a native folder picker and returns the selected path, or nil when canceled.",
    "example": "webview_choose_folder"
  },
  {
    "word": "webview_clipboard_read",
    "aliases": ["webview", "shell"],
    "group": "system",
    "stack": "-> result(string)",
    "body": "Reads text from the OS clipboard.",
    "example": "webview_clipboard_read"
  },
  {
    "word": "webview_clipboard_write",
    "aliases": ["webview", "shell"],
    "group": "system",
    "stack": "text:string -> result(bool)",
    "body": "Writes text to the OS clipboard.",
    "example": "\"Copied from Ricochet\" webview_clipboard_write"
  },
  {
    "word": "webview_open_url",
    "aliases": ["webview", "shell"],
    "group": "system",
    "stack": "url:string -> result(bool)",
    "body": "Opens a URL in the operating system's external URL handler.",
    "example": "\"https://try.ricochet.today\" webview_open_url"
  },
  {
    "word": "inspect",
    "aliases": [],
    "group": "inspect",
    "stack": "value -> value string",
    "body": "Pushes a debug representation without consuming the original value.",
    "example": "$settings inspect println"
  },
  {
    "word": "debug",
    "aliases": [],
    "group": "inspect",
    "stack": "value -> value",
    "body": "Prints a debug representation without changing the stack.",
    "example": "$payload debug"
  },
  {
    "word": "type",
    "aliases": [],
    "group": "inspect",
    "stack": "value -> string",
    "body": "Pushes the runtime value kind.",
    "example": "array type"
  },
  {
    "word": "class_of",
    "aliases": [],
    "group": "inspect",
    "stack": "value -> class",
    "body": "Pushes the built-in or instance class.",
    "example": "user class_of"
  },
  {
    "word": "instance_of?",
    "aliases": [],
    "group": "inspect",
    "stack": "value class|string -> bool",
    "body": "Checks built-in class equality or OOP inheritance.",
    "example": "user User instance_of?"
  },
  {
    "word": "responds_to?",
    "aliases": [],
    "group": "inspect",
    "stack": "method:string receiver -> bool",
    "body": "Checks whether a receiver has a built-in, native, or bytecode method.",
    "example": "\"displayName\" user responds_to?"
  },
  {
    "word": "id",
    "aliases": [],
    "group": "inspect",
    "stack": "task -> number",
    "body": "Returns a spawned task handle's numeric id.",
    "example": "$task id"
  },
  {
    "word": "info",
    "aliases": [],
    "group": "inspect",
    "stack": "task -> map",
    "body": "Returns a task metadata map with `id`, `status`, `pending`, `running`, `completed`, and `failed` fields.",
    "example": "$task info"
  },
  {
    "word": "pending?",
    "aliases": [],
    "group": "inspect",
    "stack": "task -> bool",
    "body": "Returns true while a task is still running and not yet completed or failed.",
    "example": "$task pending?"
  },
  {
    "word": "running?",
    "aliases": [],
    "group": "inspect",
    "stack": "task -> bool",
    "body": "Returns true while a spawned task is actively running.",
    "example": "$task running?"
  },
  {
    "word": "completed?",
    "aliases": [],
    "group": "inspect",
    "stack": "task -> bool",
    "body": "Returns true after a spawned task has completed successfully.",
    "example": "$task completed?"
  },
  {
    "word": "failed?",
    "aliases": [],
    "group": "inspect",
    "stack": "task -> bool",
    "body": "Returns true after a spawned task has failed.",
    "example": "$task failed?"
  },
  {
    "word": "fields",
    "aliases": [],
    "group": "inspect",
    "stack": "class|instance -> array",
    "body": "Returns inherited field names.",
    "example": "User fields"
  },
  {
    "word": "methods",
    "aliases": [],
    "group": "inspect",
    "stack": "class|instance -> set",
    "body": "Returns known native and bytecode method names.",
    "example": "User methods"
  },
  {
    "word": "callable?",
    "aliases": [],
    "group": "inspect",
    "stack": "value -> bool",
    "body": "Returns true for first-class blocks and classes.",
    "example": "[ 42 ] callable?"
  }
];

const PACKAGE_WORDS = [];

const ALL_WORDS = WORDS.concat(PACKAGE_WORDS);

const groupLabels = {
  "stack": "Stack",
  "math": "Math",
  "data": "Data",
  "collection": "Collections",
  "string": "Strings",
  "oop": "OOP",
  "control": "Control",
  "web": "Web",
  "result": "Result",
  "system": "System",
  "inspect": "Introspection"
};

const grid = document.querySelector("#word-grid");
const search = document.querySelector("#word-search");
const filterButtons = Array.from(document.querySelectorAll(".filter-button"));
let activeFilter = "all";

function renderWords() {
  const query = search.value.trim().toLowerCase();
  const visible = ALL_WORDS.filter((entry) => {
    const matchesFilter = activeFilter === "all" || entry.group === activeFilter;
    const haystack = [
      entry.word,
      entry.group,
      entry.stack,
      entry.body,
      entry.example,
      ...entry.aliases
    ].join(" ").toLowerCase();
    return matchesFilter && (!query || haystack.includes(query));
  });

  grid.innerHTML = visible.map((entry) => `
    <article class="word-card">
      <header>
        <h3><code>${escapeHtml(entry.word)}</code></h3>
        <span class="tag">${groupLabels[entry.group]}</span>
      </header>
      <div class="stack-effect">${escapeHtml(entry.stack)}</div>
      <p>${inlineCode(entry.body)}</p>
      <pre><code>${escapeHtml(entry.example)}</code></pre>
    </article>
  `).join("");

  if (visible.length === 0) {
    grid.innerHTML = `<p class="empty-state">No words match this filter yet.</p>`;
  }
}

function escapeHtml(value) {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll("\"", "&quot;");
}

function inlineCode(value) {
  return escapeHtml(value).replace(/`([^`]+)`/g, "<code>$1</code>");
}

if (grid && search) {
  filterButtons.forEach((button) => {
    button.addEventListener("click", () => {
      activeFilter = button.dataset.filter;
      filterButtons.forEach((item) => item.classList.toggle("active", item === button));
      renderWords();
    });
  });

  search.addEventListener("input", renderWords);
}

document.querySelectorAll(".copy-button").forEach((button) => {
  button.addEventListener("click", async () => {
    const code = button.parentElement.querySelector("code");
    if (!code) {
      return;
    }
    try {
      await navigator.clipboard.writeText(code.textContent);
      const old = button.textContent;
      button.textContent = "done";
      window.setTimeout(() => {
        button.textContent = old;
      }, 900);
    } catch {
      button.textContent = "nope";
      window.setTimeout(() => {
        button.textContent = "copy";
      }, 900);
    }
  });
});

if (grid && search) {
  renderWords();
}
