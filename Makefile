APP_DATA := $(HOME)/Library/Application Support/com.madison.chalk

.PHONY: setup dev cleardb clearcache

## First-time setup: install all dependencies
setup:
	npm install

## Run the app in development mode
dev:
	npm run tauri dev

## Remove all SQLite databases (main + cache)
cleardb:
	rm -f "$(APP_DATA)"/chalk.db*
	rm -f "$(APP_DATA)"/cache.db*

## Full clean: remove cache DB + onboarding status for a fresh state
clearcache: cleardb
	rm -f onboarding_status.json
