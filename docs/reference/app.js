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
    "stack": "-> array",
    "body": "Pushes a new empty array.",
    "example": "array \"Ada\" !push"
  },
  {
    "word": "!push",
    "aliases": [],
    "group": "data",
    "stack": "array value -> array",
    "body": "Appends a value to an array and returns the updated array.",
    "example": "array \"Ada\" !push \"Grace\" !push"
  },
  {
    "word": "map",
    "aliases": [],
    "group": "data",
    "stack": "-> map",
    "body": "Pushes a new empty map.",
    "example": "map \"email\" \"ada@example.com\" !put"
  },
  {
    "word": "!put",
    "aliases": [],
    "group": "data",
    "stack": "map key:string value:any -> map",
    "body": "Sets a string key on a map and returns the updated map.",
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
    "example": "map \"ok\" true !put json"
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
    "word": ".insert",
    "aliases": ["Active Record"],
    "group": "web",
    "stack": "attributes:map ModelClass -> result(record)",
    "body": "Inserts a row using mapped non-id fields and returns the inserted record.",
    "example": "map \"email\" \"ada@example.com\" !put User .insert"
  },
  {
    "word": ".update",
    "aliases": ["Active Record"],
    "group": "web",
    "stack": "id attributes:map ModelClass -> result(record)",
    "body": "Updates a row by id using mapped non-id fields and returns the updated record.",
    "example": "42 map \"email\" \"grace@example.com\" !put User .update"
  }
];

const groupLabels = {
  "stack": "Stack",
  "math": "Math",
  "data": "Data",
  "oop": "OOP",
  "control": "Control",
  "web": "Web",
  "result": "Result"
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
