# Agent System Design

## Core Purpose
The knowledge graph is for **the AI to remember what it has learned from conversations**. This includes:
- Topics discussed (AI's knowledge)
- Information the user has shared about themselves
- Facts about the user's preferences, projects, setup, etc.

## Agent Roles

### Researcher
- Searches the knowledge graph for context on a topic
- The graph contains what the AI has learned, including user-shared info

### Actor  
- Responds to the user based on its knowledge + graph context
- Maintains conversational, friendly tone

### Reviewer
- Extracts new facts from the conversation to remember
- Records both topic knowledge AND user-shared information
- Focus: What should the AI remember?

## Important Distinctions

✅ CORRECT: "User works with Rust" (user shared this)
✅ CORRECT: "discussed Rust memory management" (AI learned topic)
❌ WRONG: Recording user info that's not actually shared in conversation
