-- scripts/log_mean.lua
local frame_count = 0

system.on_pre_render(function()
    frame_count = frame_count + 1
    if frame_count % 300 == 0 then
        local mean = system.reduce_result("source_mean")
        if mean ~= nil then
            print(string.format(
                "[NeoUtl][lua] frame=%d source_mean r=%.4f g=%.4f b=%.4f a=%.4f",
                frame_count, mean[1], mean[2], mean[3], mean[4]
            ))
        end
    end
end)
