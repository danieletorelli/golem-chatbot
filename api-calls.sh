#!/bin/bash

set -euo pipefail

GOLEM_API_HOST="http://golem-chatbot.localhost:9006"

function prompt() {
  PROMPT="${1?}"
  curl -Ss -H "Accept: application/json" -X POST "${GOLEM_API_HOST}/conversations/1/prompts" -d "{\"input\":\"${PROMPT}\"}"
}

function history() {
  curl -Ss -H "Accept: application/json" -X GET "${GOLEM_API_HOST}/conversations/1"
}

if [ "${BASH_SOURCE[0]}" == "${0}" ]; then
  if [ $# -lt 1 ]; then
    echo "Usage: $0 <prompt, history> [args...]"
    exit 1
  fi

  case ${1} in
    prompt)
      prompt "${2?}"
      ;;
    history)
      history
      ;;
    *)
      echo "Invalid argument: ${1}"
      exit 1
      ;;
  esac
fi