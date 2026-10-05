Team ids (`MiniGameView::teams`, `Vitals::team`, `MiniGameRequest::SetTeam`,
`TeamEdit::id`) are a game's team slots, 1 to 64, never 0 ("No team" in a Team
condition); `SavedBuild::minigame` teams carry their `id`, and a loaded build's
teams come back in those slots.
