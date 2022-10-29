local lib = require 'mwebsocket'
local client = lib.newClient()
client:connect('ws://127.0.0.1:8080/live')

function love.update()
    -- poll message
    while true do
        local msg = client:poll()
        if not msg then break end
        print('message: ' .. msg)
    end
    -- other stuff
    -- ...
end
