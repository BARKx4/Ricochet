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
    "example": "User .all assert-ok"
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
    "example": "User .all dup ok?"
  },
  {
    "word": "drop",
    "aliases": [],
    "group": "stack",
    "stack": "a ->",
    "body": "Removes the top stack value.",
    "example": "self .name get dup nil? if drop self .email get end"
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
    "example": "users array\n\"Ada\" users get .push! drop"
  },
  {
    "word": "!push",
    "aliases": [".push!"],
    "group": "data",
    "stack": "array value -> array",
    "body": "Legacy array append word. Prefer receiver-style `.push!` for new code.",
    "example": "array \"Ada\" !push"
  },
  {
    "word": "map",
    "aliases": [],
    "group": "data",
    "stack": "name?:string -> | -> map",
    "body": "With a name on top, declares a mutable map variable. With no name, pushes a new empty map. Anonymous construction is also available as `Map new`.",
    "example": "settings map\n\"theme\" \"dark\" settings get .put! drop"
  },
  {
    "word": "!put",
    "aliases": [".put!"],
    "group": "data",
    "stack": "map key:string value:any -> map",
    "body": "Legacy map mutation word. Prefer receiver-style `.put!` for new code.",
    "example": "map \"name\" \"Ada\" !put"
  },
  {
    "word": "var",
    "aliases": [],
    "group": "data",
    "stack": "value? name:string ->",
    "body": "Declares a variable. If a value is below the name, that value becomes the initial value; otherwise it starts as nil. Existing variables are not overwritten.",
    "example": "\"Ada\" name var"
  },
  {
    "word": "get",
    "aliases": [],
    "group": "data",
    "stack": "name:string -> value | receiver member -> value",
    "body": "Reads a variable by name string, or reads a member selector from an instance/map.",
    "example": "name get\nuser .email get"
  },
  {
    "word": "set",
    "aliases": [],
    "group": "data",
    "stack": "value name:string -> | value receiver member -> updatedReceiver",
    "body": "Updates an existing variable or returns an updated instance/map with a member value changed.",
    "example": "\"Ada\" name set\n\"ada@example.com\" user .email set"
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
    "example": "self .name get nil?"
  },
  {
    "word": "subclass",
    "aliases": [],
    "group": "oop",
    "stack": "className:string superclass:class|string ->",
    "body": "Creates a class. Static declarations use `User Model subclass`; runtime declarations can use strings and variables.",
    "example": "User Model subclass\nend\n\n\"Widget\" \"Object\" subclass"
  },
  {
    "word": "field",
    "aliases": [],
    "group": "oop",
    "stack": "class:string|class fieldName:string ->",
    "body": "Adds a field to a runtime class. Inside a class body, use `name field`.",
    "example": "User \"email\" field"
  },
  {
    "word": "table",
    "aliases": [],
    "group": "oop",
    "stack": "class:string|class tableName:string ->",
    "body": "Sets a model table name. Inside a class body, use `users table`.",
    "example": "User \"users\" table"
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
    "example": "self .email get"
  },
  {
    "word": "!method",
    "aliases": [],
    "group": "oop",
    "stack": "class:string|class methodName:string block ->",
    "body": "Installs a bytecode method on a class. In class bodies, it is the preferred block method declaration operator.",
    "example": "\"displayName\" [ self .email get ] !method"
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
    "word": ".name",
    "aliases": ["dot word"],
    "group": "oop",
    "stack": "receiver -> result | receiver member get/set",
    "body": "Dot words compile to method/member selectors. Use `.email get` for fields and `.displayName` for methods.",
    "example": "user .email get\nuser .displayName"
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
    "word": "method",
    "aliases": [],
    "group": "control",
    "stack": "class-body declaration",
    "body": "Declares a named method inside a class body. Top-level methods are not supported.",
    "example": "displayName method\n  self .email get\nend"
  },
  {
    "word": "return",
    "aliases": [],
    "group": "control",
    "stack": "value -> returns value",
    "body": "Returns early from the current bytecode function or method.",
    "example": "self .name get return"
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
    "example": "payload map\n\"ok\" true payload get .put! drop\npayload get json"
  },
  {
    "word": "route",
    "aliases": ["GET", "POST"],
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
    "example": "User .all dup ok? if value else error end"
  },
  {
    "word": "value",
    "aliases": [],
    "group": "result",
    "stack": "okResult -> value",
    "body": "Unwraps an ok result. Fails loudly if the result is an error.",
    "example": "User .all dup ok? if value users var end"
  },
  {
    "word": "error",
    "aliases": [],
    "group": "result",
    "stack": "errorResult -> map",
    "body": "Unwraps an error result into a map with `kind` and `message`.",
    "example": "User .all dup ok? if value else error .message get end"
  },
  {
    "word": ".all",
    "aliases": ["Active Record"],
    "group": "web",
    "stack": "ModelClass -> result(array)",
    "body": "Active Record class method installed for mapped model classes.",
    "example": "User .all"
  },
  {
    "word": ".find",
    "aliases": ["Active Record"],
    "group": "web",
    "stack": "id ModelClass -> result(record|nil)",
    "body": "Finds a row by id for a mapped model class.",
    "example": "42 User .find"
  },
  {
    "word": ".where",
    "aliases": ["Active Record"],
    "group": "web",
    "stack": "field:string value:any ModelClass -> result(array)",
    "body": "Runs an equality query against a mapped model field.",
    "example": "\"email\" \"ada@example.com\" User .where"
  },
  {
    "word": ".limit",
    "aliases": ["Active Record"],
    "group": "web",
    "stack": "count:number ModelClass -> result(array)",
    "body": "Loads at most `count` rows from a mapped model class.",
    "example": "10 User .limit"
  },
  {
    "word": ".count",
    "aliases": ["Active Record"],
    "group": "web",
    "stack": "ModelClass -> result(number)",
    "body": "Counts rows for a mapped model class.",
    "example": "User .count"
  },
  {
    "word": ".first",
    "aliases": ["Active Record"],
    "group": "web",
    "stack": "ModelClass -> result(record|nil)",
    "body": "Loads the first row for a mapped model class.",
    "example": "User .first"
  },
  {
    "word": ".exists?",
    "aliases": ["Active Record"],
    "group": "web",
    "stack": "id ModelClass -> result(bool)",
    "body": "Checks whether a mapped row exists by id.",
    "example": "1 User .exists?"
  },
  {
    "word": ".insert",
    "aliases": ["Active Record"],
    "group": "web",
    "stack": "attributes:map ModelClass -> result(record)",
    "body": "Inserts a row using mapped non-id fields and returns the inserted record.",
    "example": "attributes map\n\"email\" \"ada@example.com\" attributes get .put! drop\nattributes get User .insert"
  },
  {
    "word": ".update",
    "aliases": ["Active Record"],
    "group": "web",
    "stack": "id attributes:map ModelClass -> result(record)",
    "body": "Updates a row by id using mapped non-id fields and returns the updated record.",
    "example": "updates map\n\"email\" \"grace@example.com\" updates get .put! drop\n42 updates get User .update"
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
    "example": "queue list\n1 queue get .push! drop"
  },
  {
    "word": "Set",
    "aliases": ["Set new"],
    "group": "collection",
    "stack": "name?:string -> | -> Class(Set)",
    "body": "With a name on top, declares a mutable set variable. With no name, pushes the Set class for `Set new`.",
    "example": "tags Set\n\"rco\" tags get .push! drop"
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
    "word": ".push!",
    "aliases": ["!push"],
    "group": "collection",
    "stack": "value collection -> sameCollection",
    "body": "Mutates an array, list, or set in place and returns the same collection for chaining.",
    "example": "\"Ada\" users get .push! \"Grace\" swap .push! drop"
  },
  {
    "word": ".put!",
    "aliases": ["!put"],
    "group": "collection",
    "stack": "key:string value:any map -> sameMap",
    "body": "Mutates a map in place and returns the same map for chaining.",
    "example": "\"theme\" \"dark\" settings get .put! drop"
  },
  {
    "word": ".insert!",
    "aliases": [],
    "group": "collection",
    "stack": "index:number value:any array|list -> sameCollection",
    "body": "Inserts at a zero-based index.",
    "example": "1 \"Lin\" users get .insert! drop"
  },
  {
    "word": ".remove!",
    "aliases": [],
    "group": "collection",
    "stack": "value:any collection -> sameCollection | key:string map -> sameMap",
    "body": "Removes a matching value from arrays/lists/sets or a key from maps.",
    "example": "\"theme\" settings get .remove! drop"
  },
  {
    "word": ".remove-at!",
    "aliases": [],
    "group": "collection",
    "stack": "index:number array|list -> sameCollection",
    "body": "Removes the value at a zero-based index.",
    "example": "0 users get .remove-at! drop"
  },
  {
    "word": ".clear!",
    "aliases": [],
    "group": "collection",
    "stack": "collection -> sameCollection",
    "body": "Clears a mutable collection in place.",
    "example": "users get .clear! drop"
  },
  {
    "word": ".count",
    "aliases": [".length"],
    "group": "collection",
    "stack": "string|collection -> number",
    "body": "Counts characters for strings or items for collections.",
    "example": "users get .count"
  },
  {
    "word": ".at",
    "aliases": [],
    "group": "collection",
    "stack": "index:number string|array|list -> value | key:string map -> value",
    "body": "Reads an indexed value, character, or map entry. Missing values produce nil.",
    "example": "0 users get .at\n\"theme\" settings get .at"
  },
  {
    "word": ".first",
    "aliases": [],
    "group": "collection",
    "stack": "string|array|list|set -> value|nil",
    "body": "Returns the first character or item, or nil for an empty receiver.",
    "example": "users get .first"
  },
  {
    "word": ".last",
    "aliases": [],
    "group": "collection",
    "stack": "string|array|list|set -> value|nil",
    "body": "Returns the last character or item, or nil for an empty receiver.",
    "example": "users get .last"
  },
  {
    "word": ".take",
    "aliases": [],
    "group": "collection",
    "stack": "count:number string|array|list|set -> sameKind",
    "body": "Returns the first count characters or items.",
    "example": "2 users get .take"
  },
  {
    "word": ".skip",
    "aliases": [],
    "group": "collection",
    "stack": "count:number string|array|list|set -> sameKind",
    "body": "Returns characters or items after the first count entries.",
    "example": "1 users get .skip"
  },
  {
    "word": ".reverse",
    "aliases": [],
    "group": "collection",
    "stack": "string|array|list|set -> sameKind",
    "body": "Returns characters or items in reverse order.",
    "example": "users get .reverse"
  },
  {
    "word": ".has?",
    "aliases": [],
    "group": "collection",
    "stack": "value:any collection -> bool | key:string map -> bool",
    "body": "Checks membership or map-key presence.",
    "example": "\"theme\" settings get .has?"
  },
  {
    "word": ".keys",
    "aliases": [],
    "group": "collection",
    "stack": "map -> array",
    "body": "Returns map keys as an array of strings.",
    "example": "settings get .keys"
  },
  {
    "word": ".values",
    "aliases": [],
    "group": "collection",
    "stack": "map -> array",
    "body": "Returns map values as an array.",
    "example": "settings get .values"
  },
  {
    "word": ".each",
    "aliases": [],
    "group": "collection",
    "stack": "block collection -> sameCollection",
    "body": "Runs a block for each item. Map blocks receive key then value.",
    "example": "[ println ] users get .each drop"
  },
  {
    "word": ".transform",
    "aliases": [],
    "group": "collection",
    "stack": "block collection -> array",
    "body": "Maps each item through a block and returns an array.",
    "example": "[ 2 * ] numbers get .transform"
  },
  {
    "word": ".select",
    "aliases": [],
    "group": "collection",
    "stack": "block collection -> collection",
    "body": "Keeps items whose block result is truthy.",
    "example": "[ 4 > ] numbers get .select"
  },
  {
    "word": ".reduce",
    "aliases": [],
    "group": "collection",
    "stack": "initial:any block array|list|set -> value",
    "body": "Reduces a sequence by calling the block with accumulator then item.",
    "example": "0 [ + ] numbers get .reduce"
  },
  {
    "word": ".find",
    "aliases": [],
    "group": "collection",
    "stack": "block collection -> value|nil",
    "body": "Returns the first item whose block result is truthy.",
    "example": "[ 8 = ] numbers get .find"
  },
  {
    "word": ".any?",
    "aliases": [],
    "group": "collection",
    "stack": "block collection -> bool",
    "body": "Returns true if any item matches.",
    "example": "[ 10 = ] numbers get .any?"
  },
  {
    "word": ".all?",
    "aliases": [],
    "group": "collection",
    "stack": "block collection -> bool",
    "body": "Returns true if every item matches.",
    "example": "[ 0 > ] numbers get .all?"
  },
  {
    "word": ".join",
    "aliases": [],
    "group": "collection",
    "stack": "separator:string collection -> string",
    "body": "Joins a collection of displayable values into a string.",
    "example": "\", \" users get .join"
  },
  {
    "word": ".trim",
    "aliases": [],
    "group": "string",
    "stack": "string -> string",
    "body": "Trims leading and trailing whitespace.",
    "example": "\" Ada \" .trim"
  },
  {
    "word": ".trim-start",
    "aliases": [],
    "group": "string",
    "stack": "string -> string",
    "body": "Trims leading whitespace.",
    "example": "\"  Ada\" .trim-start"
  },
  {
    "word": ".trim-end",
    "aliases": [],
    "group": "string",
    "stack": "string -> string",
    "body": "Trims trailing whitespace.",
    "example": "\"Ada  \" .trim-end"
  },
  {
    "word": ".blank?",
    "aliases": [],
    "group": "string",
    "stack": "string -> bool",
    "body": "Returns true when a string is empty or only whitespace.",
    "example": "\"  \" .blank?"
  },
  {
    "word": ".slice",
    "aliases": [],
    "group": "string",
    "stack": "start:number count:number string -> string",
    "body": "Returns count characters starting at start.",
    "example": "2 4 \"ricochet\" .slice"
  },
  {
    "word": ".index-of",
    "aliases": [],
    "group": "string",
    "stack": "needle:string string -> number|nil",
    "body": "Returns the first character index for a substring, or nil.",
    "example": "\"co\" \"ricochet\" .index-of"
  },
  {
    "word": ".last-index-of",
    "aliases": [],
    "group": "string",
    "stack": "needle:string string -> number|nil",
    "body": "Returns the last character index for a substring, or nil.",
    "example": "\"c\" \"ricochet\" .last-index-of"
  },
  {
    "word": ".repeat",
    "aliases": [],
    "group": "string",
    "stack": "count:number string -> string",
    "body": "Repeats a string count times.",
    "example": "3 \"ha\" .repeat"
  },
  {
    "word": ".lines",
    "aliases": [],
    "group": "string",
    "stack": "string -> array",
    "body": "Splits a string into lines.",
    "example": "\"a\\nb\" .lines"
  },
  {
    "word": ".chars",
    "aliases": [],
    "group": "string",
    "stack": "string -> array",
    "body": "Splits a string into one-character strings.",
    "example": "\"cat\" .chars"
  },
  {
    "word": ".split",
    "aliases": [],
    "group": "string",
    "stack": "separator:string string -> array",
    "body": "Splits a string.",
    "example": "\",\" \"Ada,Grace\" .split"
  },
  {
    "word": ".replace",
    "aliases": [],
    "group": "string",
    "stack": "needle:string replacement:string string -> string",
    "body": "Replaces all matching substrings.",
    "example": "\"telnet\" \"web\" \"telnet era\" .replace"
  },
  {
    "word": ".contains?",
    "aliases": [],
    "group": "string",
    "stack": "needle:string string -> bool",
    "body": "Checks substring presence.",
    "example": "\"co\" \"Ricochet\" .contains?"
  },
  {
    "word": ".starts-with?",
    "aliases": [],
    "group": "string",
    "stack": "prefix:string string -> bool",
    "body": "Checks a string prefix.",
    "example": "\"Rico\" \"Ricochet\" .starts-with?"
  },
  {
    "word": ".ends-with?",
    "aliases": [],
    "group": "string",
    "stack": "suffix:string string -> bool",
    "body": "Checks a string suffix.",
    "example": "\"chet\" \"Ricochet\" .ends-with?"
  },
  {
    "word": ".uppercase",
    "aliases": [],
    "group": "string",
    "stack": "string -> string",
    "body": "Converts to uppercase.",
    "example": "\"ricochet\" .uppercase"
  },
  {
    "word": ".lowercase",
    "aliases": [],
    "group": "string",
    "stack": "string -> string",
    "body": "Converts to lowercase.",
    "example": "\"RICOCHET\" .lowercase"
  },
  {
    "word": ".concat",
    "aliases": [],
    "group": "string",
    "stack": "suffix:string string -> string",
    "body": "Concatenates two strings.",
    "example": "\"chet\" \"Rico\" .concat"
  },
  {
    "word": ".to-number",
    "aliases": [],
    "group": "string",
    "stack": "string -> result(number)",
    "body": "Parses an integer and returns a stack result.",
    "example": "\"42\" .to-number value"
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
    "word": ".matches?",
    "aliases": ["regex"],
    "group": "string",
    "stack": "haystack:string regex -> bool",
    "body": "Returns true when the regex matches the string.",
    "example": "\"hello-world\" slugPattern get .matches?"
  },
  {
    "word": ".find",
    "aliases": ["regex"],
    "group": "string",
    "stack": "haystack:string regex -> map|nil",
    "body": "Returns a match map with `text`, `start`, and `end`, or nil.",
    "example": "\"abc123\" digits get .find"
  },
  {
    "word": ".captures",
    "aliases": ["regex"],
    "group": "string",
    "stack": "haystack:string regex -> map|nil",
    "body": "Returns numbered and named capture groups as a map, or nil.",
    "example": "\"item-42\" pairPattern get .captures"
  },
  {
    "word": ".replace",
    "aliases": ["regex"],
    "group": "string",
    "stack": "haystack:string replacement:string regex -> string",
    "body": "Replaces all regex matches.",
    "example": "\"abc123\" \"#\" digits get .replace"
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
    "word": ".error?",
    "aliases": [],
    "group": "result",
    "stack": "result -> bool",
    "body": "Returns true for error results.",
    "example": "result get .error?"
  },
  {
    "word": ".unwrap-or",
    "aliases": [],
    "group": "result",
    "stack": "fallback:any result -> value",
    "body": "Returns the ok value or a fallback.",
    "example": "\"guest\" maybeName get .unwrap-or"
  },
  {
    "word": ".map-result",
    "aliases": [],
    "group": "result",
    "stack": "block result -> result",
    "body": "Transforms an ok value and passes error results through unchanged.",
    "example": "[ 2 * ] 21 ok .map-result value"
  },
  {
    "word": ".and-then",
    "aliases": [],
    "group": "result",
    "stack": "block result -> result",
    "body": "Runs a block that must itself return a result when the receiver is ok.",
    "example": "[ ok ] value get .and-then"
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
    "example": "args .count"
  },
  {
    "word": "env",
    "aliases": [],
    "group": "system",
    "stack": "name:string -> result(string)",
    "body": "Reads an environment variable as a result.",
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
    "word": "fs",
    "aliases": [],
    "group": "system",
    "stack": "-> filesystemCapability",
    "body": "Pushes the filesystem capability when the host enabled it.",
    "example": "\"README.md\" fs .read-text"
  },
  {
    "word": ".read-text",
    "aliases": [],
    "group": "system",
    "stack": "path:string fs -> result(string)",
    "body": "Reads a UTF-8 text file.",
    "example": "\"README.md\" fs .read-text value"
  },
  {
    "word": ".write-text!",
    "aliases": [],
    "group": "system",
    "stack": "path:string contents:string fs -> result(path)",
    "body": "Writes a UTF-8 text file.",
    "example": "\"out.txt\" \"hello\" fs .write-text!"
  },
  {
    "word": ".exists?",
    "aliases": [],
    "group": "system",
    "stack": "path:string fs -> bool",
    "body": "Checks file or directory existence.",
    "example": "\"README.md\" fs .exists?"
  },
  {
    "word": ".list",
    "aliases": [],
    "group": "system",
    "stack": "path:string fs -> result(array)",
    "body": "Lists directory entries as path strings.",
    "example": "\".\" fs .list value"
  },
  {
    "word": ".create-dir!",
    "aliases": [],
    "group": "system",
    "stack": "path:string fs -> result(path)",
    "body": "Creates a directory and parents if needed.",
    "example": "\"tmp/cache\" fs .create-dir!"
  },
  {
    "word": "http",
    "aliases": [],
    "group": "system",
    "stack": "-> httpCapability",
    "body": "Pushes the HTTP client capability when the host enabled it.",
    "example": "\"http://127.0.0.1:3000\" http .get"
  },
  {
    "word": ".get",
    "aliases": ["HTTP"],
    "group": "system",
    "stack": "url:string http -> result(map)",
    "body": "Runs an HTTP GET and returns a result map with status, body, and headers.",
    "example": "\"https://example.com\" http .get value"
  },
  {
    "word": ".post-json",
    "aliases": [],
    "group": "system",
    "stack": "url:string body:any http -> result(map)",
    "body": "Posts a JSON-encoded Ricochet value.",
    "example": "\"https://api.example\" payload get http .post-json"
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
