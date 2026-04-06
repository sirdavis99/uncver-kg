# uncverkg Skill

## Purpose
Use this skill when you need to manage project knowledge using the uncverkg knowledge graph engine.

## Setup
- The project has `kg.json` in the root - uncverkg auto-detects this
- Knowledge is stored in `uncver-kg-kg/` (unique per project, gitignored)

## Commands

### Write a fact
```bash
uncverkg write --subject "Subject" --predicate "RELATION" --object "Object"
```

### Bulk write from JSON file
```bash
uncverkg bulk --file facts/my-facts.json
```

### Read/search nodes
```bash
uncverkg read --query "search term"
```

### Update a node
```bash
uncverkg update --id "UUID" --label "New Label"
```

### Start interactive chat with context
```bash
uncverkg chat
```

## Knowledge Graph Standard

### Node Format
Nodes are stored as facts: `Subject --PREDICATE--> Object`

### Properties
- `subject`: The entity being described
- `predicate`: The relationship/action
- `object`: The target of the relationship

### Example
```json
{"subject": "David", "predicate": "WORKS_ON", "object": "Rust"}
```

## When to Use
- User shares information about themselves or their project
- Learning new facts about code, architecture, or decisions
- Tracking project context across sessions
- Storing user preferences or settings
