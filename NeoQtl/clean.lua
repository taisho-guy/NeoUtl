local io_open = io.open
local string_find = string.find
local string_sub = string.sub
local table_concat = table.concat

local EXCLUDE_DIRS = {
    "target", ".git", ".svn", ".hg", "neoutl-wgpu", "node_modules",
    "dist", "build", "out", ".next", ".nuxt", "__pycache__", ".venv", "venv",
    ".idea", ".vscode", "slang"
}

local FAMILIES = {
    c_like = { quotes = {'"', "'"}, line = "//", block_open = "/*", block_close = "*/" },
    rust   = { quotes = {'"'}, line = "//", block_open = "/*", block_close = "*/", raw = true },
    hash   = { quotes = {'"', "'"}, line = "#" },
    sql    = { quotes = {"'"}, line = "--", block_open = "/*", block_close = "*/" },
    html   = { quotes = {'"', "'"}, block_open = "<!--", block_close = "-->" },
    ini    = { quotes = {'"'}, line = ";" },
    data   = { quotes = {'"'} },
}

local EXT_FAMILY = {
    c="c_like", h="c_like", cpp="c_like", hpp="c_like", cc="c_like", hh="c_like",
    cxx="c_like", hxx="c_like", inl="c_like", ipp="c_like",
    cs="c_like", java="c_like", kt="c_like", kts="c_like", scala="c_like", sc="c_like",
    swift="c_like", go="c_like", d="c_like", zig="c_like", dart="c_like", groovy="c_like",
    js="c_like", jsx="c_like", mjs="c_like", cjs="c_like", ts="c_like", tsx="c_like",
    mts="c_like", cts="c_like", css="c_like", scss="c_like", less="c_like",
    glsl="c_like", frag="c_like", vert="c_like", geom="c_like", comp="c_like",
    tesc="c_like", tese="c_like", hlsl="c_like", wgsl="c_like", metal="c_like", shader="c_like",
    php="c_like", phtml="c_like", qml="c_like", proto="c_like", graphql="c_like", gql="c_like",
    rs="rust", slang="rust",
    py="hash", pyw="hash", sh="hash", bash="hash", zsh="hash", fish="hash",
    rb="hash", rake="hash", pl="hash", pm="hash", r="hash", nim="hash",
    yaml="hash", yml="hash", toml="hash",
    sql="sql", prisma="sql", surrealql="sql",
    html="html", htm="html", xhtml="html", xml="html", vue="html", svelte="html",
    ini="ini", cfg="ini", properties="ini",
    json="data", json5="data", jsonc="data",
}

local IS_WINDOWS = os.getenv("OS") and os.getenv("OS"):match("[Ww]indows") or os.getenv("WINDIR") ~= nil

local function normalize_path(path)
    return path:gsub("\\", "/")
end

local function pattern_class(chars)
    local seen, out = {}, {}
    for _, c in ipairs(chars) do
        if not seen[c] then
            seen[c] = true
            if c:match("[%%%^%]%-]") then
                out[#out+1] = "%" .. c
            else
                out[#out+1] = c
            end
        end
    end
    return "[" .. table_concat(out) .. "]"
end

local function build_jump(fam)
    local chars = {}
    for _, q in ipairs(fam.quotes or {}) do chars[#chars+1] = q end
    if fam.line then chars[#chars+1] = fam.line:sub(1,1) end
    if fam.block_open then chars[#chars+1] = fam.block_open:sub(1,1) end
    if fam.raw then chars[#chars+1] = "r" end
    return pattern_class(chars)
end

for _, fam in pairs(FAMILIES) do
    fam.jump = build_jump(fam)
end

local function skip_quote(content, len, i, q)
    i = i + 1
    while i <= len do
        local _, end_idx = string_find(content, q, i, true)
        if not end_idx then return len + 1 end
        local esc, chk = 0, end_idx - 1
        while chk >= i and string_sub(content, chk, chk) == '\\' do
            esc = esc + 1
            chk = chk - 1
        end
        i = end_idx + 1
        if esc % 2 == 0 then return i end
    end
    return i
end

local function skip_raw_string(content, len, i)
    local n2 = string_sub(content, i+1, i+1)
    if n2 == '"' then
        i = i + 2
        local _, end_idx = string_find(content, '"', i, true)
        return end_idx and (end_idx + 1) or (len + 1)
    elseif n2 == '#' then
        local _, sharp_end = string_find(content, '"', i + 2, true)
        if not sharp_end then return i + 1 end
        local sharps = string_sub(content, i+1, sharp_end-1)
        local close = '"' .. sharps
        local _, end_idx = string_find(content, close, sharp_end, true)
        return end_idx and (end_idx + #close) or (len + 1)
    end
    return i + 1
end

local function clean_generic(content, fam)
    local len = #content
    local result, r_idx, last_pos, i = {}, 1, 1, 1

    while i <= len do
        local next_idx = string_find(content, fam.jump, i)
        if not next_idx then break end
        i = next_idx
        local c1 = string_sub(content, i, i)
        local is_quote = false

        for _, q in ipairs(fam.quotes or {}) do
            if c1 == q then
                is_quote = true
                i = skip_quote(content, len, i, q)
                break
            end
        end

        if not is_quote then
            if fam.raw and c1 == 'r' then
                i = skip_raw_string(content, len, i)
            elseif fam.block_open and string_sub(content, i, i + #fam.block_open - 1) == fam.block_open then
                result[r_idx] = string_sub(content, last_pos, i - 1)
                r_idx = r_idx + 1
                local _, e = string_find(content, fam.block_close, i + #fam.block_open, true)
                i = e and (e + 1) or (len + 1)
                last_pos = i
            elseif fam.line and string_sub(content, i, i + #fam.line - 1) == fam.line then
                result[r_idx] = string_sub(content, last_pos, i - 1)
                r_idx = r_idx + 1
                local _, e = string_find(content, "\n", i + #fam.line, true)
                i = e and (e + 1) or (len + 1)
                last_pos = i
            else
                i = i + 1
            end
        end
    end

    if r_idx > 1 then
        result[r_idx] = string_sub(content, last_pos, len)
        return table_concat(result)
    end
    return nil
end

local LUA_JUMP = '[%-%"%\'%[]'

local function clean_lua(content)
    local len = #content
    local result, r_idx, last_pos, i = {}, 1, 1, 1

    while i <= len do
        local next_idx = string_find(content, LUA_JUMP, i)
        if not next_idx then break end
        i = next_idx
        local b1 = string_sub(content, i, i)

        if b1 == '"' or b1 == "'" then
            i = skip_quote(content, len, i, b1)
        elseif b1 == '[' then
            if string_sub(content, i+1, i+1) == '[' then
                local _, end_idx = string_find(content, ']]', i + 2, true)
                i = end_idx and (end_idx + 2) or (len + 1)
            else
                i = i + 1
            end
        elseif b1 == '-' then
            if string_sub(content, i+1, i+1) == '-' then
                result[r_idx] = string_sub(content, last_pos, i - 1)
                r_idx = r_idx + 1
                if string_sub(content, i+2, i+3) == '[[' then
                    local _, e = string_find(content, "]]", i + 4, true)
                    i = e and (e + 2) or (len + 1)
                else
                    local _, e = string_find(content, "\n", i + 2, true)
                    i = e and (e + 1) or (len + 1)
                end
                last_pos = i
            else
                i = i + 1
            end
        else
            i = i + 1
        end
    end

    if r_idx > 1 then
        result[r_idx] = string_sub(content, last_pos, len)
        return table_concat(result)
    end
    return nil
end

local function clean_comments(content, ext)
    if ext == "lua" then return clean_lua(content) end
    local family = EXT_FAMILY[ext]
    if not family then return nil end
    return clean_generic(content, FAMILIES[family])
end

local function remove_comments_from_file(filepath, ext)
    local file = io_open(filepath, "rb")
    if not file then return end
    local content = file:read("*all")
    file:close()

    local cleaned = clean_comments(content, ext)
    if cleaned then
        local wfile = io_open(filepath, "wb")
        if wfile then
            wfile:write(cleaned)
            wfile:close()
            print("Cleaned (" .. ext .. "): " .. filepath)
        end
    end
end

local function pattern_escape(s)
    return s:gsub("[%^%$%(%)%%%.%[%]%*%+%-%?]", "%%%1")
end

local function is_excluded(filepath)
    local norm_path = normalize_path(filepath)
    for _, dir in ipairs(EXCLUDE_DIRS) do
        local d = pattern_escape(dir)
        if norm_path:find("/" .. d .. "/") or norm_path:find("^%.?/?" .. d .. "/") then
            return true
        end
    end
    return false
end

local function target_extensions()
    local exts = { lua = true }
    for ext, _ in pairs(EXT_FAMILY) do exts[ext] = true end
    return exts
end

local function build_find_cmd()
    local parts = { "find . -type f \\(" }
    local first = true
    for ext, _ in pairs(target_extensions()) do
        if not first then parts[#parts+1] = "-o" end
        parts[#parts+1] = '-name "*.' .. ext .. '"'
        first = false
    end
    parts[#parts+1] = "\\) -print"
    return table_concat(parts, " ")
end

local function scan_project()
    local exts = target_extensions()
    local cmd
    if IS_WINDOWS then
        cmd = 'dir /b /s /a-d 2>nul'
    else
        cmd = build_find_cmd()
    end

    local p = io.popen(cmd)
    if not p then return end

    for raw_file in p:lines() do
        local file = normalize_path(raw_file)
        if file ~= "" and not is_excluded(file) then
            local ext = file:match("%.([^%.]+)$")
            if ext then
                ext = ext:lower()
                if ext == "lua" or exts[ext] then
                    remove_comments_from_file(file, ext)
                end
            end
        end
    end
    p:close()
end

scan_project()
