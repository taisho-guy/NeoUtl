local io_open = io.open
local string_find = string.find
local string_sub = string.sub
local table_concat = table.concat

-- ==========================================
-- 1. 各言語ごとのパース高速ジャンプ用パターン
-- ==========================================
local EXT_CONFIGS = {
    rs = { jump = '[%/r%"]' },    -- Rust: //, /* */, ", r", r#", URL除外
    lua = { jump = '[%-%"%\'%[]' }, -- Lua: --, --[[ ]], ", ', [[ ]], URL除外
    slang = { jump = '[%/r%"]' },  -- Slang: Rust/C系と同様の構文
    json = { jump = '[%"]' },       -- JSON: コメントなし、文字列のみ保護（念のため）
    toml = { jump = '[%#%"%\']' },   -- TOML: #, ", ', """
    yaml = { jump = '[%#%"%\']' }    -- YAML: #, ", '
}

-- ==========================================
-- 2. 究極高速化パースエンジン
-- ==========================================
local function clean_comments(content, ext)
    local len = #content
    local result = {}
    local r_idx = 1
    local last_pos = 1
    local i = 1
    local config = EXT_CONFIGS[ext]

    while i <= len do
        local next_idx = string_find(content, config.jump, i)
        if not next_idx then break end
        
        i = next_idx
        local b1 = string_sub(content, i, i)

        -- ------------------------------------------
        -- 【共通】通常の文字列リテラル保護 ("..." や '...')
        -- ------------------------------------------
        if b1 == '"' or b1 == "'" then
            local q = b1
            i = i + 1
            while i <= len do
                local _, end_idx = string_find(content, q, i, true)
                if not end_idx then i = len + 1 break end
                
                -- エスケープ文字（\" や \'）の逆算
                local esc_count = 0
                local check_idx = end_idx - 1
                while check_idx >= i and string_sub(content, check_idx, check_idx) == '\\' do
                    esc_count = esc_count + 1
                    check_idx = check_idx - 1
                end
                
                if esc_count % 2 == 0 then
                    i = end_idx + 1
                    break
                else
                    i = end_idx + 1
                end
            end

        -- ------------------------------------------
        -- 【Rust / Slang】生文字列リテラル & C系コメント
        -- ------------------------------------------
        elseif (ext == "rs" or ext == "slang") and b1 == 'r' then
            local n2 = string_sub(content, i+1, i+2)
            if string_sub(n2, 1, 1) == '"' then
                i = i + 2
                local _, end_idx = string_find(content, '"', i, true)
                i = end_idx and (end_idx + 1) or (len + 1)
            elseif string_sub(n2, 1, 1) == '#' then
                local _, start_sharp_end = string_find(content, '"', i + 2, true)
                if start_sharp_end then
                    local sharps = string_sub(content, i + 1, start_sharp_end - 1)
                    local close_pattern = '"' .. sharps
                    local _, end_idx = string_find(content, close_pattern, start_sharp_end, true)
                    i = end_idx and (end_idx + #close_pattern) or (len + 1)
                else
                    i = i + 1
                end
            else
                i = i + 1
            end

        elseif (ext == "rs" or ext == "slang") and b1 == '/' then
            local b2 = string_sub(content, i+1, i+1)
            if b2 == '/' then
                result[r_idx] = string_sub(content, last_pos, i - 1)
                r_idx = r_idx + 1
                local _, e = string_find(content, "\n", i + 2, true)
                i = e and (e + 1) or (len + 1)
                last_pos = i
            elseif b2 == '*' then
                result[r_idx] = string_sub(content, last_pos, i - 1)
                r_idx = r_idx + 1
                local _, e = string_find(content, "*/", i + 2, true)
                i = e and (e + 2) or (len + 1)
                last_pos = i
            else
                i = i + 1
            end

        -- ------------------------------------------
        -- 【Lua】ロングストリング & Luaコメント
        -- ------------------------------------------
        elseif ext == "lua" and b1 == '[' then
            -- [[ ... ]] の検知（シャープ等がない純粋なリテラル開始のみ対応）
            if string_sub(content, i+1, i+1) == '[' then
                local _, end_idx = string_find(content, ']]', i + 2, true)
                i = end_idx and (end_idx + 2) or (len + 1)
            else
                i = i + 1
            end

        elseif ext == "lua" and b1 == '-' then
            if string_sub(content, i+1, i+1) == '-' then
                result[r_idx] = string_sub(content, last_pos, i - 1)
                r_idx = r_idx + 1
                
                -- --[[ 複数行コメント ]] の判定
                if string_sub(content, i+2, i+3) == '[[' then
                    local _, e = string_find(content, "]]", i + 4, true)
                    i = e and (e + 2) or (len + 1)
                else
                    -- 通常の1行コメント
                    local _, e = string_find(content, "\n", i + 2, true)
                    i = e and (e + 1) or (len + 1)
                end
                last_pos = i
            else
                i = i + 1
            end

        -- ------------------------------------------
        -- 【TOML / YAML】# コメント
        -- ------------------------------------------
        elseif (ext == "toml" or ext == "yaml") and b1 == '#' then
            result[r_idx] = string_sub(content, last_pos, i - 1)
            r_idx = r_idx + 1
            local _, e = string_find(content, "\n", i + 1, true)
            i = e and (e + 1) or (len + 1)
            last_pos = i

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

-- ==========================================
-- 3. ファイル処理・OSコマンド実行
-- ==========================================
local function remove_comments_from_file(filepath, ext)
    local file = io_open(filepath, "r")
    if not file then return end
    local content = file:read("*all")
    file:close()

    local cleaned = clean_comments(content, ext)
    if cleaned then
        local wfile = io_open(filepath, "w")
        if wfile then
            wfile:write(cleaned)
            wfile:close()
            print("Cleaned (" .. ext .. "): " .. filepath)
        end
    end
end

local function scan_project()
    -- 拡張子指定で対象を絞る（target, .git を弾きつつ高速find）
    local cmd = (os.getenv("WINDIR") or os.getenv("windir"))
        and 'dir /b /s *.rs *.lua *.slang *.json *.toml *.yaml 2>nul' 
        or 'find . -type d -name "target" -prune -o -type d -name ".git" -prune -o -type f \\( -name "*.rs" -o -name "*.lua" -o -name "*.slang" -o -name "*.json" -o -name "*.toml" -o -name "*.yaml" \\) -print'

    local p = io.popen(cmd)
    if not p then return end

    for file in p:lines() do
        if file ~= "" then
            local ext = string_sub(file, #file - 3) -- .xxx
            ext = ext:match("%.([^%.]+)$") or file:match("%.([^%.]+)$")
            if ext and EXT_CONFIGS[ext] then
                remove_comments_from_file(file, ext)
            end
        end
    end
    p:close()
end

scan_project()
