# clix × Gemini — integration guide

clix capabilities map directly to Gemini function declarations. The `sideEffectClass` and `risk` fields in pack manifests provide the right metadata for safe tool use.

---

## Get function declarations

```sh
# All capabilities as Gemini function_declarations
clix tools export --format gemini

# Scoped to a namespace
clix tools export --format gemini --namespace gcloud.aiplatform

# All capabilities flat
clix tools export --format gemini --all
```

Gemini uses `OBJECT`, `STRING`, `INTEGER` (uppercase) — `clix tools export` converts automatically.

---

## Python integration (google-generativeai SDK)

```python
import subprocess, json
import google.generativeai as genai
from google.generativeai.types import FunctionDeclaration, Tool

def get_clix_tools(namespace: str | None = None) -> list[Tool]:
    cmd = ["clix", "tools", "export", "--format", "gemini"]
    if namespace:
        cmd += ["--namespace", namespace]
    output = subprocess.run(cmd, capture_output=True, text=True).stdout
    data = json.loads(output)

    declarations = [
        FunctionDeclaration(
            name=d["name"],
            description=d["description"],
            parameters=d["parameters"],
        )
        for d in data["function_declarations"]
    ]
    return [Tool(function_declarations=declarations)]

def run_clix_tool(tool_name: str, tool_args: dict) -> dict:
    # Convert double-underscore back to dot-separated capability name
    cap_name = tool_name.replace("__", ".")
    cmd = ["clix", "run", cap_name, "--json"]
    for k, v in tool_args.items():
        cmd += ["-i", f"{k}={v}"]
    result = subprocess.run(cmd, capture_output=True, text=True)
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError:
        return {"error": result.stderr or result.stdout}

def run_agent(user_message: str, namespace: str | None = None) -> str:
    genai.configure(api_key="YOUR_API_KEY")
    tools = get_clix_tools(namespace)
    model = genai.GenerativeModel(model_name="gemini-2.0-flash", tools=tools)
    chat = model.start_chat()

    response = chat.send_message(user_message)

    while response.candidates[0].finish_reason.name == "STOP":
        # Check for function calls
        function_calls = [
            part.function_call
            for part in response.parts
            if part.function_call.name
        ]
        if not function_calls:
            break

        # Execute each function call and send results back
        function_responses = []
        for fc in function_calls:
            result = run_clix_tool(fc.name, dict(fc.args))
            function_responses.append(
                genai.protos.Part(
                    function_response=genai.protos.FunctionResponse(
                        name=fc.name,
                        response={"result": result},
                    )
                )
            )
        response = chat.send_message(function_responses)

    return response.text

# Usage
print(run_agent(
    "List all Vertex AI models in project my-project region us-central1",
    namespace="gcloud.aiplatform"
))
```

---

## Python integration (google-genai SDK, newer)

```python
import subprocess, json
from google import genai
from google.genai import types

def get_function_declarations(namespace: str | None = None):
    cmd = ["clix", "tools", "export", "--format", "gemini"]
    if namespace:
        cmd += ["--namespace", namespace]
    output = subprocess.run(cmd, capture_output=True, text=True).stdout
    data = json.loads(output)
    return data["function_declarations"]

def run_clix(tool_name: str, tool_args: dict) -> str:
    cap = tool_name.replace("__", ".")
    cmd = ["clix", "run", cap, "--json"]
    for k, v in tool_args.items():
        cmd += ["-i", f"{k}={v}"]
    result = subprocess.run(cmd, capture_output=True, text=True)
    return result.stdout or result.stderr

client = genai.Client(api_key="YOUR_API_KEY")

declarations = get_function_declarations(namespace="gcloud.aiplatform")
tools = types.Tool(function_declarations=declarations)
config = types.GenerateContentConfig(tools=[tools])

response = client.models.generate_content(
    model="gemini-2.0-flash",
    contents="List Vertex AI models in project my-project, region us-central1",
    config=config,
)

# Handle function calls
for part in response.candidates[0].content.parts:
    if part.function_call:
        fc = part.function_call
        result = run_clix(fc.name, dict(fc.args))
        print(f"Tool {fc.name} returned:\n{result}")
```

---

## Tool name format

Gemini tool names must match `[a-zA-Z0-9_]+`. clix converts dot-separated names to double-underscore:

```
git.status              → git__status
gcloud.aiplatform.models.list → gcloud__aiplatform__models__list
```

When routing a Gemini function call back to clix, convert back:

```python
cap_name = tool_name.replace("__", ".")   # git__status → git.status
```

---

## Namespace scoping

Gemini has no namespace stub pattern (unlike MCP). For large catalogues, scope by namespace at model init time:

```python
# Git agent — only see git capabilities
tools = get_clix_tools(namespace="git")

# kubectl agent — only kubectl
tools = get_clix_tools(namespace="kubectl")
```

This keeps the function declaration list small and improves tool selection accuracy.

---

## Receipt audit

All executions are receipted regardless of which SDK integration runs them:

```sh
clix receipts list --json
```
