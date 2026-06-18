# @ricochet/forms

Beta helpers for form-shaped maps and validation results.

The package provides small stack words for form field maps, required-string
checks, schema-shaped validation, validation error maps, validation result maps,
and multipart/upload helper maps.

```ricochet
"forms/validation" import

"email" "ada@example.com" form_field
"value" at
```

## Schema Validation

Use `form_schema` to build a map of field rules, then pass form data and the
schema to `form_schema_validate`.

```ricochet
"forms/validation" import

form_schema
  "email" "string" form_required_field
  "age" "number" form_optional_field
schema var

data map
data get "email" "ada@example.com" put! drop

data get schema get form_schema_validate result var
result get "ok" at
```

Supported rule types are `string`, `number`, `bool`, `map`, `array`, and `any`.
Required strings reject blank values; optional fields are ignored when missing.
