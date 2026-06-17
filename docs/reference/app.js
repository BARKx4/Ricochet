const WORDS = [
  {
    "word": "+",
    "aliases": ["add"],
    "group": "math",
    "stack": "left:number right:number -> sum:number",
    "body": "Adds two numbers. The VM checks integer overflow and leaves the stack untouched on failure.",
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
    "body": "Subtracts the right number from the left number. The VM checks integer overflow and preserves operands on failure.",
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
    "word": "not-equals?",
    "aliases": ["!="],
    "group": "math",
    "stack": "left:any right:any -> bool",
    "body": "Returns true when two values are not equal.",
    "example": "\"Ada\" \"Grace\" not-equals?"
  },
  {
    "word": "!=",
    "aliases": ["not-equals?"],
    "group": "math",
    "stack": "left:any right:any -> bool",
    "body": "Symbol alias for `not-equals?`.",
    "example": "1 2 !="
  },
  {
    "word": "assert-equals",
    "aliases": [],
    "group": "math",
    "stack": "actual:any expected:any ->",
    "body": "Fails the current VM run when actual and expected differ. Used by `rco test`.",
    "example": "\"Ada\" \"Ada\" assert-equals"
  },
  {
    "word": "assert",
    "aliases": [],
    "group": "math",
    "stack": "value ->",
    "body": "Consumes a truthy value or fails the current VM run.",
    "example": "email get empty? not assert"
  },
  {
    "word": "assert-true",
    "aliases": [],
    "group": "math",
    "stack": "bool ->",
    "body": "Consumes true or fails the current VM run.",
    "example": "saved get assert-true"
  },
  {
    "word": "assert-false",
    "aliases": [],
    "group": "math",
    "stack": "bool ->",
    "body": "Consumes false or fails the current VM run.",
    "example": "deleted get assert-false"
  },
  {
    "word": "assert-ok",
    "aliases": [],
    "group": "result",
    "stack": "result ->",
    "body": "Consumes an ok result or fails the current VM run.",
    "example": "User all assert-ok"
  },
  {
    "word": "assert-error",
    "aliases": [],
    "group": "result",
    "stack": "result ->",
    "body": "Consumes an error result or fails the current VM run.",
    "example": "\"Validation\" \"bad\" fail assert-error"
  },
  {
    "word": "less-than?",
    "aliases": ["<"],
    "group": "math",
    "stack": "left:number right:number -> bool",
    "body": "Numeric less-than comparison.",
    "example": "3 5 less-than?"
  },
  {
    "word": "<",
    "aliases": ["less-than?"],
    "group": "math",
    "stack": "left:number right:number -> bool",
    "body": "Symbol alias for `less-than?`.",
    "example": "3 5 <"
  },
  {
    "word": "greater-than?",
    "aliases": [">"],
    "group": "math",
    "stack": "left:number right:number -> bool",
    "body": "Numeric greater-than comparison.",
    "example": "8 4 greater-than?"
  },
  {
    "word": ">",
    "aliases": ["greater-than?"],
    "group": "math",
    "stack": "left:number right:number -> bool",
    "body": "Symbol alias for `greater-than?`.",
    "example": "8 4 >"
  },
  {
    "word": "less-or-equals?",
    "aliases": ["<="],
    "group": "math",
    "stack": "left:number right:number -> bool",
    "body": "Numeric less-than-or-equal comparison.",
    "example": "5 5 less-or-equals?"
  },
  {
    "word": "<=",
    "aliases": ["less-or-equals?"],
    "group": "math",
    "stack": "left:number right:number -> bool",
    "body": "Symbol alias for `less-or-equals?`.",
    "example": "5 5 <="
  },
  {
    "word": "greater-or-equals?",
    "aliases": [">="],
    "group": "math",
    "stack": "left:number right:number -> bool",
    "body": "Numeric greater-than-or-equal comparison.",
    "example": "5 5 greater-or-equals?"
  },
  {
    "word": ">=",
    "aliases": ["greater-or-equals?"],
    "group": "math",
    "stack": "left:number right:number -> bool",
    "body": "Symbol alias for `greater-or-equals?`.",
    "example": "5 5 >="
  },
  {
    "word": "swap",
    "aliases": [],
    "group": "stack",
    "stack": "a b -> b a",
    "body": "Swaps the top two stack values.",
    "example": "ctx get \"home/index\" swap view"
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
    "example": "users array\nusers get \"Ada\" push! drop"
  },
  {
    "word": "push!",
    "aliases": [],
    "group": "data",
    "stack": "collection value -> collection",
    "body": "Appends a value to a mutable array, list, or set and returns the collection.",
    "example": "users get \"Ada\" push! drop"
  },
  {
    "word": "map",
    "aliases": [],
    "group": "data",
    "stack": "name?:string -> | -> map",
    "body": "With a name on top, declares a mutable map variable. With no name, pushes a new empty map. Anonymous construction is also available as `Map new`.",
    "example": "settings map\nsettings get \"theme\" \"dark\" put! drop"
  },
  {
    "word": "put!",
    "aliases": [],
    "group": "data",
    "stack": "map key:string value:any -> map",
    "body": "Sets a key on a mutable map and returns the map.",
    "example": "settings get \"name\" \"Ada\" put! drop"
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
    "body": "Reads a variable by name string. For map/object keyed access, use `container key at`; for generated object accessors, use `field.get` selectors.",
    "example": "name get\nrequest get \"method\" at\nuser email.get"
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
    "body": "Installs a bytecode method inside the current class body.",
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
    "example": "( left right -> Number ) sum function\n  left get right get +\nend"
  },
  {
    "word": "Method",
    "aliases": [],
    "group": "control",
    "stack": "class-body declaration",
    "body": "Declares a named method inside a class body. Top-level methods are not supported.",
    "example": "[ self email.get ] \"displayName\" Method"
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
    "example": "task get await"
  },
  {
    "word": "await-all",
    "aliases": [],
    "group": "control",
    "stack": "array|list -> array",
    "body": "Awaits an array or list of task handles and returns their results in input order. Completed handles reuse their cached values.",
    "example": "handles get await-all"
  },
  {
    "word": "release-task",
    "aliases": [],
    "group": "control",
    "stack": "task -> bool",
    "body": "Releases an awaited completed or failed task handle from the current VM's retained task table. Running tasks must be awaited before release.",
    "example": "task get await\ntask get release-task"
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
    "word": "while",
    "aliases": ["end"],
    "group": "control",
    "stack": "conditionExpression -> repeated body",
    "body": "Re-executes the condition expression before every iteration and runs the body while it remains truthy.",
    "example": "count get 10 < while\n  count get 1 + count set\nend"
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
    "example": "ctx get \"home/index\" swap view"
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
    "body": "Builds a JSON controller response from nil, bool, number, string, array, or map values.",
    "example": "payload map\npayload get \"ok\" true put! drop\npayload get json"
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
    "word": "find",
    "aliases": ["Active Record"],
    "group": "web",
    "stack": "id ModelClass -> result(record|nil)",
    "body": "Finds a row by id for a mapped model class.",
    "example": "42 User find"
  },
  {
    "word": "default-page",
    "aliases": ["Active Record"],
    "group": "web",
    "stack": "ModelClass -> result(array) | modelName:string DatabaseCapability -> result(array)",
    "body": "Loads the v1 beta default list page: up to 50 rows, ordered by `id asc` when the model maps an `id` field, otherwise the first bounded page.",
    "example": "User default-page\n\"User\" db get default-page"
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
    "word": "count",
    "aliases": ["Active Record"],
    "group": "web",
    "stack": "ModelClass -> result(number)",
    "body": "Counts rows for a mapped model class.",
    "example": "User count"
  },
  {
    "word": "first",
    "aliases": ["Active Record"],
    "group": "web",
    "stack": "ModelClass -> result(record|nil)",
    "body": "Loads the first row for a mapped model class.",
    "example": "User first"
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
    "example": "attributes map\nattributes get \"email\" \"ada@example.com\" put! drop\nattributes get User insert"
  },
  {
    "word": "update",
    "aliases": ["Active Record"],
    "group": "web",
    "stack": "id attributes:map ModelClass -> result(record)",
    "body": "Updates a row by id using mapped non-id fields and returns the updated record.",
    "example": "updates map\nupdates get \"email\" \"grace@example.com\" put! drop\n42 updates get User update"
  },
  {
    "word": "*",
    "aliases": ["multiply"],
    "group": "math",
    "stack": "left:number right:number -> product:number",
    "body": "Multiplies two numbers with overflow checks.",
    "example": "6 7 *"
  },
  {
    "word": "/",
    "aliases": ["divide"],
    "group": "math",
    "stack": "left:number right:number -> quotient:number",
    "body": "Integer division. Division by zero fails loudly and preserves operands.",
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
    "body": "Negates a number with overflow checks.",
    "example": "5 negate"
  },
  {
    "word": "abs",
    "aliases": [],
    "group": "math",
    "stack": "number -> number",
    "body": "Absolute value with overflow checks.",
    "example": "0 5 - abs"
  },
  {
    "word": "min",
    "aliases": [],
    "group": "math",
    "stack": "a:number b:number -> number",
    "body": "Returns the smaller number.",
    "example": "3 7 min"
  },
  {
    "word": "max",
    "aliases": [],
    "group": "math",
    "stack": "a:number b:number -> number",
    "body": "Returns the larger number.",
    "example": "3 7 max"
  },
  {
    "word": "clamp",
    "aliases": [],
    "group": "math",
    "stack": "value:number min:number max:number -> number",
    "body": "Clamps a number into an inclusive range.",
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
    "example": "queue list\nqueue get 1 push! drop"
  },
  {
    "word": "Set",
    "aliases": ["Set new"],
    "group": "collection",
    "stack": "name?:string -> | -> Class(Set)",
    "body": "With a name on top, declares a mutable set variable. With no name, pushes the Set class for `Set new`.",
    "example": "tags Set\ntags get \"rco\" push! drop"
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
    "word": "push!",
    "aliases": [],
    "group": "collection",
    "stack": "collection value -> sameCollection",
    "body": "Mutates an array, list, or set in place and returns the same collection for chaining.",
    "example": "users get \"Ada\" push! \"Grace\" push! drop"
  },
  {
    "word": "put!",
    "aliases": [],
    "group": "collection",
    "stack": "map key:string value:any -> sameMap",
    "body": "Mutates a map in place and returns the same map for chaining.",
    "example": "settings get \"theme\" \"dark\" put! drop"
  },
  {
    "word": "insert!",
    "aliases": [],
    "group": "collection",
    "stack": "index:number value:any array|list -> sameCollection",
    "body": "Inserts at a zero-based index.",
    "example": "users get 1 \"Lin\" insert! drop"
  },
  {
    "word": "remove!",
    "aliases": [],
    "group": "collection",
    "stack": "value:any collection -> sameCollection | key:string map -> sameMap",
    "body": "Removes a matching value from arrays/lists/sets or a key from maps.",
    "example": "settings get \"theme\" remove! drop"
  },
  {
    "word": "remove-at!",
    "aliases": [],
    "group": "collection",
    "stack": "index:number array|list -> sameCollection",
    "body": "Removes the value at a zero-based index.",
    "example": "users get 0 remove-at! drop"
  },
  {
    "word": "clear!",
    "aliases": [],
    "group": "collection",
    "stack": "collection -> sameCollection",
    "body": "Clears a mutable collection in place.",
    "example": "users get clear! drop"
  },
  {
    "word": "count",
    "aliases": ["length"],
    "group": "collection",
    "stack": "string|collection -> number",
    "body": "Counts characters for strings or items for collections.",
    "example": "users get count"
  },
  {
    "word": "at",
    "aliases": [],
    "group": "collection",
    "stack": "index:number string|array|list -> value | key:string map -> value",
    "body": "Reads an indexed value, character, or map entry. Missing values produce nil.",
    "example": "users get 0 at\nsettings get \"theme\" at"
  },
  {
    "word": "first",
    "aliases": [],
    "group": "collection",
    "stack": "string|array|list|set -> value|nil",
    "body": "Returns the first character or item, or nil for an empty receiver.",
    "example": "users get first"
  },
  {
    "word": "last",
    "aliases": [],
    "group": "collection",
    "stack": "string|array|list|set -> value|nil",
    "body": "Returns the last character or item, or nil for an empty receiver.",
    "example": "users get last"
  },
  {
    "word": "take",
    "aliases": [],
    "group": "collection",
    "stack": "count:number string|array|list|set -> sameKind",
    "body": "Returns the first count characters or items.",
    "example": "users get 2 take"
  },
  {
    "word": "skip",
    "aliases": [],
    "group": "collection",
    "stack": "count:number string|array|list|set -> sameKind",
    "body": "Returns characters or items after the first count entries.",
    "example": "users get 1 skip"
  },
  {
    "word": "reverse",
    "aliases": [],
    "group": "collection",
    "stack": "string|array|list|set -> sameKind",
    "body": "Returns characters or items in reverse order.",
    "example": "users get reverse"
  },
  {
    "word": "has?",
    "aliases": [],
    "group": "collection",
    "stack": "value:any collection -> bool | key:string map -> bool",
    "body": "Checks membership or map-key presence.",
    "example": "settings get \"theme\" has?"
  },
  {
    "word": "keys",
    "aliases": [],
    "group": "collection",
    "stack": "map -> array",
    "body": "Returns map keys as an array of strings.",
    "example": "settings get keys"
  },
  {
    "word": "values",
    "aliases": [],
    "group": "collection",
    "stack": "map -> array",
    "body": "Returns map values as an array.",
    "example": "settings get values"
  },
  {
    "word": "each",
    "aliases": [],
    "group": "collection",
    "stack": "block collection -> sameCollection",
    "body": "Runs a block for each item. Map blocks receive key then value.",
    "example": "[ println ] users get each drop"
  },
  {
    "word": "transform",
    "aliases": [],
    "group": "collection",
    "stack": "block collection -> array",
    "body": "Maps each item through a block and returns an array.",
    "example": "[ 2 * ] numbers get transform"
  },
  {
    "word": "select",
    "aliases": [],
    "group": "collection",
    "stack": "block collection -> collection",
    "body": "Keeps items whose block result is truthy.",
    "example": "[ 4 > ] numbers get select"
  },
  {
    "word": "reduce",
    "aliases": [],
    "group": "collection",
    "stack": "initial:any block array|list|set -> value",
    "body": "Reduces a sequence by calling the block with accumulator then item.",
    "example": "0 [ + ] numbers get reduce"
  },
  {
    "word": "find",
    "aliases": [],
    "group": "collection",
    "stack": "block collection -> value|nil",
    "body": "Returns the first item whose block result is truthy.",
    "example": "[ 8 = ] numbers get find"
  },
  {
    "word": "any?",
    "aliases": [],
    "group": "collection",
    "stack": "block collection -> bool",
    "body": "Returns true if any item matches.",
    "example": "[ 10 = ] numbers get any?"
  },
  {
    "word": "all?",
    "aliases": [],
    "group": "collection",
    "stack": "block collection -> bool",
    "body": "Returns true if every item matches.",
    "example": "[ 0 > ] numbers get all?"
  },
  {
    "word": "join",
    "aliases": [],
    "group": "collection",
    "stack": "separator:string collection -> string",
    "body": "Joins a collection of displayable values into a string.",
    "example": "users get \", \" join"
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
    "word": "trim-start",
    "aliases": [],
    "group": "string",
    "stack": "string -> string",
    "body": "Trims leading whitespace.",
    "example": "\"  Ada\" trim-start"
  },
  {
    "word": "trim-end",
    "aliases": [],
    "group": "string",
    "stack": "string -> string",
    "body": "Trims trailing whitespace.",
    "example": "\"Ada  \" trim-end"
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
    "word": "index-of",
    "aliases": [],
    "group": "string",
    "stack": "needle:string string -> number|nil",
    "body": "Returns the first character index for a substring, or nil.",
    "example": "\"ricochet\" \"co\" index-of"
  },
  {
    "word": "last-index-of",
    "aliases": [],
    "group": "string",
    "stack": "needle:string string -> number|nil",
    "body": "Returns the last character index for a substring, or nil.",
    "example": "\"ricochet\" \"c\" last-index-of"
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
    "body": "Replaces all matching substrings.",
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
    "word": "starts-with?",
    "aliases": [],
    "group": "string",
    "stack": "prefix:string string -> bool",
    "body": "Checks a string prefix.",
    "example": "\"Ricochet\" \"Rico\" starts-with?"
  },
  {
    "word": "ends-with?",
    "aliases": [],
    "group": "string",
    "stack": "suffix:string string -> bool",
    "body": "Checks a string suffix.",
    "example": "\"Ricochet\" \"chet\" ends-with?"
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
    "word": "to-number",
    "aliases": [],
    "group": "string",
    "stack": "string -> result(number)",
    "body": "Parses an integer and returns a stack result.",
    "example": "\"42\" to-number value"
  },
  {
    "word": "to-string",
    "aliases": [],
    "group": "string",
    "stack": "value -> string",
    "body": "Converts any value to its display string.",
    "example": "42 to-string"
  },
  {
    "word": "json-encode",
    "aliases": [],
    "group": "string",
    "stack": "value -> string",
    "body": "Encodes nil, bool, number, string, array, list, set, map, or result values as JSON.",
    "example": "settings get json-encode"
  },
  {
    "word": "json-decode",
    "aliases": [],
    "group": "string",
    "stack": "string -> result(value)",
    "body": "Decodes JSON into Ricochet values.",
    "example": "\"{\\\"ok\\\":true}\" json-decode value"
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
    "example": "\"hello-world\" slugPattern get matches?"
  },
  {
    "word": "find",
    "aliases": ["regex"],
    "group": "string",
    "stack": "haystack:string regex -> map|nil",
    "body": "Returns a match map with `text`, `start`, and `end`, or nil.",
    "example": "\"abc123\" digits get find"
  },
  {
    "word": "captures",
    "aliases": ["regex"],
    "group": "string",
    "stack": "haystack:string regex -> map|nil",
    "body": "Returns numbered and named capture groups as a map, or nil.",
    "example": "\"item-42\" pairPattern get captures"
  },
  {
    "word": "replace",
    "aliases": ["regex"],
    "group": "string",
    "stack": "haystack:string replacement:string regex -> string",
    "body": "Replaces all regex matches.",
    "example": "\"abc123\" \"#\" digits get replace"
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
    "example": "result get error?"
  },
  {
    "word": "unwrap-or",
    "aliases": [],
    "group": "result",
    "stack": "fallback:any result -> value",
    "body": "Returns the ok value or a fallback.",
    "example": "maybeName get \"guest\" unwrap-or"
  },
  {
    "word": "map-result",
    "aliases": [],
    "group": "result",
    "stack": "block result -> result",
    "body": "Transforms an ok value and passes error results through unchanged.",
    "example": "21 ok [ 2 * ] map-result value"
  },
  {
    "word": "and-then",
    "aliases": [],
    "group": "result",
    "stack": "block result -> result",
    "body": "Runs a block that must itself return a result when the receiver is ok.",
    "example": "value get [ ok ] and-then"
  },
  {
    "word": "result_envelope",
    "aliases": [],
    "group": "result",
    "stack": "result options:map -> map",
    "body": "Converts a Result into a shared `{ ok, data, error, meta }` map for app and API boundaries. The options map becomes `meta`; when it contains a non-empty string `capability`, error envelopes also include `error.capability`. Error `code` currently mirrors the Result kind.",
    "example": "options map\noptions get \"capability\" \"workspace.read\" put! drop\n\"payload\" ok options get result_envelope"
  },
  {
    "word": "print",
    "aliases": [],
    "group": "system",
    "stack": "value ->",
    "body": "Writes to captured stdout without adding a newline.",
    "example": "\"Name: \" print name get print"
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
    "word": "read-line",
    "aliases": [],
    "group": "system",
    "stack": "-> string|nil",
    "body": "Reads one line from the installed input reader.",
    "example": "read-line name var"
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
    "word": "env",
    "aliases": [],
    "group": "system",
    "stack": "name:string -> result(string)",
    "body": "Reads an environment variable as a result when environment access is enabled. `--env-allow NAME` can narrow reads to specific variable names.",
    "example": "\"DATABASE_URL\" env"
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
    "body": "Returns a map describing enabled host capabilities such as filesystem, workspace, HTTP, process, PTY, approval, environment, sleep, TUI, and webview. Environment entries include an `allowlist` array when reads are name-bounded; process entries include the cwd root used by process and PTY launches.",
    "example": "runtime_capabilities \"environment\" at \"allowlist\" at"
  },
  {
    "word": "process_spawn",
    "aliases": ["process"],
    "group": "system",
    "stack": "command:string args:array options:map -> result(map)",
    "body": "Runs a direct child process to completion when the process capability is enabled. Options include `cwd`, `stdin`, `timeout_ms`, `clear_env`, `env`, `stdout_max_bytes`, and `stderr_max_bytes`; `cwd` is bounded by `--process-root` when configured, then by `--fs-root`. The result map includes `success`, `status`, `stdout`, `stderr`, `stdout_truncated`, and `stderr_truncated`.",
    "example": "args array\noptions map\n\"git\" args get options get process_spawn value"
  },
  {
    "word": "process_spawn_task",
    "aliases": ["process"],
    "group": "system",
    "stack": "command:string args:array options:map -> task",
    "body": "Starts `process_spawn` on a task worker. Await the task to receive the same result map returned by `process_spawn`.",
    "example": "args array\noptions map\n\"git\" args get options get process_spawn_task await value"
  },
  {
    "word": "process_start",
    "aliases": [],
    "group": "system",
    "stack": "command:string args:array options:map -> result(map)",
    "body": "Starts a direct child process as a long-running job when the process capability is enabled. The returned snapshot includes `id`, `status`, `running`, `success`, output lengths, truncation flags, timeout state, and cancellation state.",
    "example": "args array\noptions map\n\"git\" args get options get process_start value"
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
    "example": "job get \"id\" at process_job value"
  },
  {
    "word": "process_cancel",
    "aliases": [],
    "group": "system",
    "stack": "id:number -> result(map)",
    "body": "Requests cancellation for a running process job and returns the latest snapshot. Completed jobs remain inspectable.",
    "example": "job get \"id\" at process_cancel value"
  },
  {
    "word": "process_read",
    "aliases": [],
    "group": "system",
    "stack": "id:number options:map -> result(map)",
    "body": "Reads retained stdout/stderr for a process job. Options include `stdout_offset` and `stderr_offset`; the result includes output slices, next offsets, and the same snapshot fields as `process_job`.",
    "example": "readOptions map\njob get \"id\" at readOptions get process_read value"
  },
  {
    "word": "pty_start",
    "aliases": [],
    "group": "system",
    "stack": "command:string args:array options:map -> result(map)",
    "body": "Starts a command in a real pseudo-terminal when the PTY capability is enabled. Options include `cwd`, `clear_env`, `env`, `rows`, `cols`, and `output_max_bytes`.",
    "example": "args array\nargs get \"repl\" push! drop\noptions map\n\"rco\" args get options get pty_start value"
  },
  {
    "word": "pty_write",
    "aliases": [],
    "group": "system",
    "stack": "id:number input:string -> result(map)",
    "body": "Writes input to a running PTY session and returns the latest session snapshot.",
    "example": "session get \"id\" at \"1 2 +\\r\\n\" pty_write value"
  },
  {
    "word": "pty_read",
    "aliases": [],
    "group": "system",
    "stack": "id:number options:map -> result(map)",
    "body": "Reads retained PTY output. Option `offset` supports incremental reads; the result includes `output`, next `offset`, truncation state, size, status, and process metadata.",
    "example": "readOptions map\nsession get \"id\" at readOptions get pty_read value"
  },
  {
    "word": "pty_resize",
    "aliases": [],
    "group": "system",
    "stack": "id:number cols:number rows:number -> result(map)",
    "body": "Resizes a PTY session and returns the latest snapshot.",
    "example": "session get \"id\" at 120 40 pty_resize value"
  },
  {
    "word": "pty_stop",
    "aliases": [],
    "group": "system",
    "stack": "id:number options:map -> result(map)",
    "body": "Requests termination of a PTY session and returns the latest snapshot.",
    "example": "stopOptions map\nsession get \"id\" at stopOptions get pty_stop value"
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
    "example": "session get \"id\" at pty_detail value"
  },
  {
    "word": "approval_create",
    "aliases": [],
    "group": "system",
    "stack": "operation:map options:map -> result(map)",
    "body": "Creates a runtime-local approval record and returns a one-time token in the create result. Options include `id`, `token`, `ttl_ms`, `expires_at_ms`, and `metadata`; unknown options fail with `ApprovalRequestError`.",
    "example": "operation map\noptions map\noperation get options get approval_create value"
  },
  {
    "word": "approval_claim",
    "aliases": [],
    "group": "system",
    "stack": "id:string token:string -> result(map)",
    "body": "Claims a pending approval exactly once. Expired, rejected, completed, already claimed, or token-mismatched approvals return structured result errors.",
    "example": "approval get \"id\" at approval get \"token\" at approval_claim value"
  },
  {
    "word": "approval_complete",
    "aliases": [],
    "group": "system",
    "stack": "id:string result:value -> result(map)",
    "body": "Marks a claimed approval completed and stores the caller-provided result value for audit. Pending approvals must be claimed before completion.",
    "example": "approval get \"id\" at result get approval_complete value"
  },
  {
    "word": "approval_reject",
    "aliases": [],
    "group": "system",
    "stack": "id:string reason:string -> result(map)",
    "body": "Marks a pending or claimed approval rejected with a reason. Final and expired approvals cannot be rejected again.",
    "example": "approval get \"id\" at \"Rejected by user\" approval_reject value"
  },
  {
    "word": "approval_detail",
    "aliases": [],
    "group": "system",
    "stack": "id:string -> result(map)",
    "body": "Returns a retained approval record without re-exposing the one-time token after creation.",
    "example": "approval get \"id\" at approval_detail value"
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
    "word": "workspace_resolve",
    "aliases": [],
    "group": "system",
    "stack": "path:string options:map -> result(map)",
    "body": "Resolves a workspace path through the filesystem root and returns a structured map with requested path, resolved path, relative path, root containment, and existence.",
    "example": "options map\n\".\" options get workspace_resolve value"
  },
  {
    "word": "workspace_contains?",
    "aliases": [],
    "group": "system",
    "stack": "root:string path:string -> bool",
    "body": "Returns true when both paths resolve through the filesystem capability and the path is inside the resolved root.",
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
    "body": "Lists workspace entries as metadata maps. Options include `recursive`, `include_files`, `include_dirs`, and `max_entries`.",
    "example": "options map\n\".\" options get workspace_list value"
  },
  {
    "word": "workspace_read_text",
    "aliases": [],
    "group": "system",
    "stack": "path:string options:map -> result(string)",
    "body": "Reads UTF-8 workspace text with a bounded `max_bytes` option. The default cap is 1 MiB.",
    "example": "options map\n\"README.md\" options get workspace_read_text value"
  },
  {
    "word": "workspace_write_text",
    "aliases": [],
    "group": "system",
    "stack": "path:string contents:string options:map -> result(map)",
    "body": "Writes UTF-8 text through the workspace bounds when filesystem writes are enabled. Options include `overwrite` and `create_parent_dirs`; overwrite defaults to false.",
    "example": "options map\n\"out.txt\" \"hello\" options get workspace_write_text value"
  },
  {
    "word": "workspace_mkdir",
    "aliases": [],
    "group": "system",
    "stack": "path:string options:map -> result(map)",
    "body": "Creates a workspace directory when filesystem writes are enabled. Option `recursive` defaults to true.",
    "example": "options map\n\"tmp/cache\" options get workspace_mkdir value"
  },
  {
    "word": "workspace_copy",
    "aliases": [],
    "group": "system",
    "stack": "source:string destination:string options:map -> result(map)",
    "body": "Copies a file inside workspace bounds. Options include `overwrite` and `create_parent_dirs`; overwrite defaults to false.",
    "example": "options map\n\"README.md\" \"tmp/README.md\" options get workspace_copy value"
  },
  {
    "word": "workspace_move",
    "aliases": [],
    "group": "system",
    "stack": "source:string destination:string options:map -> result(map)",
    "body": "Renames a file or directory inside workspace bounds. Existing destinations are rejected.",
    "example": "options map\n\"tmp/a.txt\" \"tmp/b.txt\" options get workspace_move value"
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
    "example": "request get \"timeout_ms\" 30000 put! drop\nrequest get http_request value"
  },
  {
    "word": "http_post_json",
    "aliases": ["HTTP"],
    "group": "system",
    "stack": "url:string body:any -> result(map)",
    "body": "Posts a JSON-encoded Ricochet value.",
    "example": "\"https://api.example\" payload get http_post_json"
  },
  {
    "word": "http_post_json_task",
    "aliases": ["HTTP", "async"],
    "group": "system",
    "stack": "url:string body:any -> task",
    "body": "Starts a JSON HTTP POST on a task worker. Await the task to receive the same result map returned by `http_post_json`.",
    "example": "\"https://api.example\" payload get http_post_json_task await value"
  },
  {
    "word": "http_request_task",
    "aliases": ["HTTP", "headers", "async"],
    "group": "system",
    "stack": "request:map -> task",
    "body": "Starts a mapped HTTP request on a task worker. Await the task to receive the same result map returned by `http_request`, including request-level timeout, byte-cap, scheme, and host policy checks.",
    "example": "request get http_request_task await value"
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
    "example": "\"Counter\" $body state get actions get webview_window_state value"
  },
  {
    "word": "inspect",
    "aliases": [],
    "group": "inspect",
    "stack": "value -> value string",
    "body": "Pushes a debug representation without consuming the original value.",
    "example": "settings get inspect println"
  },
  {
    "word": "debug",
    "aliases": [],
    "group": "inspect",
    "stack": "value -> value",
    "body": "Prints a debug representation without changing the stack.",
    "example": "payload get debug"
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
    "word": "class-of",
    "aliases": [],
    "group": "inspect",
    "stack": "value -> class",
    "body": "Pushes the built-in or instance class.",
    "example": "user class-of"
  },
  {
    "word": "instance-of?",
    "aliases": [],
    "group": "inspect",
    "stack": "value class|string -> bool",
    "body": "Checks built-in class equality or OOP inheritance.",
    "example": "user User instance-of?"
  },
  {
    "word": "responds-to?",
    "aliases": [],
    "group": "inspect",
    "stack": "method:string receiver -> bool",
    "body": "Checks whether a receiver has a built-in, native, or bytecode method.",
    "example": "\"displayName\" user responds-to?"
  },
  {
    "word": "id",
    "aliases": [],
    "group": "inspect",
    "stack": "task -> number",
    "body": "Returns a spawned task handle's numeric id.",
    "example": "task get id"
  },
  {
    "word": "info",
    "aliases": [],
    "group": "inspect",
    "stack": "task -> map",
    "body": "Returns a task metadata map with `id`, `status`, `pending`, `running`, `completed`, and `failed` fields.",
    "example": "task get info"
  },
  {
    "word": "status",
    "aliases": [],
    "group": "inspect",
    "stack": "task -> string",
    "body": "Returns `running`, `completed`, `failed`, or `consumed` for a task handle.",
    "example": "task get status"
  },
  {
    "word": "pending?",
    "aliases": [],
    "group": "inspect",
    "stack": "task -> bool",
    "body": "Returns true while a task is still running and not yet completed or failed.",
    "example": "task get pending?"
  },
  {
    "word": "running?",
    "aliases": [],
    "group": "inspect",
    "stack": "task -> bool",
    "body": "Returns true while a spawned task is actively running.",
    "example": "task get running?"
  },
  {
    "word": "completed?",
    "aliases": [],
    "group": "inspect",
    "stack": "task -> bool",
    "body": "Returns true after a spawned task has completed successfully.",
    "example": "task get completed?"
  },
  {
    "word": "failed?",
    "aliases": [],
    "group": "inspect",
    "stack": "task -> bool",
    "body": "Returns true after a spawned task has failed.",
    "example": "task get failed?"
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
  const visible = WORDS.filter((entry) => {
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

filterButtons.forEach((button) => {
  button.addEventListener("click", () => {
    activeFilter = button.dataset.filter;
    filterButtons.forEach((item) => item.classList.toggle("active", item === button));
    renderWords();
  });
});

search.addEventListener("input", renderWords);

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

renderWords();
