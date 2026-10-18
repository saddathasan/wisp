-- One-off VoiceOver-readability check (never in a measurement run: it polls Wisp's AX tree).
-- usage: osascript ax_rows.applescript <pid>, with the menu open; prints each item's role, name and child texts.
on run argv
	set pid to (item 1 of argv) as integer
	tell application "System Events"
		tell (first process whose unix id is pid)
			set m to menu 1 of menu bar item 1 of menu bar 1
			set res to ""
			repeat with e in menu items of m
				set res to res & (role of e) & " | name=" & (name of e as text) & " | enabled=" & (enabled of e as text)
				try
					repeat with c in UI elements of e
						set res to res & " [" & (role of c) & ": value=" & (value of c as text) & " desc=" & (description of c as text) & "]"
					end repeat
				end try
				set res to res & linefeed
			end repeat
		end tell
	end tell
	return res
end run
