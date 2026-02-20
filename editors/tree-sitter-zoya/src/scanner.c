#include "tree_sitter/parser.h"

#include <stdbool.h>
#include <string.h>

// External token types — must match order in grammar.js externals
enum TokenType {
  INTERPOLATED_STRING_START,
  INTERPOLATED_STRING_CONTENT,
  INTERPOLATED_STRING_EXPR_START,
  INTERPOLATED_STRING_EXPR_END,
  INTERPOLATED_STRING_END,
};

// Scanner state
#define MAX_DEPTH 32

typedef struct {
  bool in_string;       // Are we inside an interpolated string?
  int brace_depth;      // Depth of {} inside interpolation expression
  int stack_len;        // Number of nested interpolation levels
  int brace_stack[MAX_DEPTH]; // Stack of brace depths for nested interpolations
} Scanner;

void *tree_sitter_zoya_external_scanner_create(void) {
  Scanner *scanner = (Scanner *)calloc(1, sizeof(Scanner));
  return scanner;
}

void tree_sitter_zoya_external_scanner_destroy(void *payload) {
  free(payload);
}

unsigned tree_sitter_zoya_external_scanner_serialize(void *payload,
                                                     char *buffer) {
  Scanner *scanner = (Scanner *)payload;
  unsigned size = 0;

  buffer[size++] = (char)scanner->in_string;
  buffer[size++] = (char)scanner->brace_depth;
  buffer[size++] = (char)scanner->stack_len;

  for (int i = 0; i < scanner->stack_len && i < MAX_DEPTH; i++) {
    buffer[size++] = (char)scanner->brace_stack[i];
  }

  return size;
}

void tree_sitter_zoya_external_scanner_deserialize(void *payload,
                                                    const char *buffer,
                                                    unsigned length) {
  Scanner *scanner = (Scanner *)payload;
  scanner->in_string = false;
  scanner->brace_depth = 0;
  scanner->stack_len = 0;

  if (length == 0) return;

  unsigned pos = 0;
  scanner->in_string = (bool)buffer[pos++];
  if (pos >= length) return;
  scanner->brace_depth = (int)buffer[pos++];
  if (pos >= length) return;
  scanner->stack_len = (int)buffer[pos++];

  for (int i = 0; i < scanner->stack_len && pos < length && i < MAX_DEPTH; i++) {
    scanner->brace_stack[i] = (int)buffer[pos++];
  }
}

static void advance(TSLexer *lexer) { lexer->advance(lexer, false); }

static void skip(TSLexer *lexer) { lexer->advance(lexer, true); }

bool tree_sitter_zoya_external_scanner_scan(void *payload, TSLexer *lexer,
                                             const bool *valid_symbols) {
  Scanner *scanner = (Scanner *)payload;

  // If we can produce INTERPOLATED_STRING_START, look for $"
  if (valid_symbols[INTERPOLATED_STRING_START]) {
    // Skip whitespace
    while (lexer->lookahead == ' ' || lexer->lookahead == '\t' ||
           lexer->lookahead == '\n' || lexer->lookahead == '\r') {
      skip(lexer);
    }

    if (lexer->lookahead == '$') {
      advance(lexer);
      if (lexer->lookahead == '"') {
        advance(lexer);
        lexer->result_symbol = INTERPOLATED_STRING_START;
        scanner->in_string = true;
        scanner->brace_depth = 0;
        return true;
      }
      return false;
    }
  }

  // If inside an interpolated string
  if (scanner->in_string) {
    // Check for expression end (closing brace of interpolation)
    if (valid_symbols[INTERPOLATED_STRING_EXPR_END] && scanner->brace_depth == 0 &&
        lexer->lookahead == '}') {
      advance(lexer);
      lexer->result_symbol = INTERPOLATED_STRING_EXPR_END;
      return true;
    }

    // Check for string end (closing quote)
    if (valid_symbols[INTERPOLATED_STRING_END] && lexer->lookahead == '"') {
      advance(lexer);
      lexer->result_symbol = INTERPOLATED_STRING_END;
      scanner->in_string = false;
      return true;
    }

    // Check for expression start (opening brace)
    if (valid_symbols[INTERPOLATED_STRING_EXPR_START] &&
        lexer->lookahead == '{') {
      advance(lexer);
      lexer->result_symbol = INTERPOLATED_STRING_EXPR_START;
      scanner->brace_depth = 0;
      return true;
    }

    // Scan string content
    if (valid_symbols[INTERPOLATED_STRING_CONTENT]) {
      bool has_content = false;

      while (lexer->lookahead != 0) {
        if (lexer->lookahead == '"' || lexer->lookahead == '{') {
          break;
        }

        if (lexer->lookahead == '\\') {
          // Consume escape sequence
          has_content = true;
          advance(lexer);
          if (lexer->lookahead != 0) {
            advance(lexer);
          }
          continue;
        }

        has_content = true;
        advance(lexer);
      }

      if (has_content) {
        lexer->result_symbol = INTERPOLATED_STRING_CONTENT;
        return true;
      }
    }
  }

  return false;
}
