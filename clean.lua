#!/usr/bin/env luajit

local LINE_COMMENT = {
    [".rs"] = "//",
    [".slint"] = "//",
    [".wgsl"] = "//",
    [".py"] = "#"
}

local NOISE_PATTERNS = {
    "^%-+$",
    "^=+$",
    "^(here|this)%s",
    "^(this function|this method|this struct|this class)%s",
    "^(create|creates|initialize|initializes|construct|constructs|declare|declares)%s+(a|the|an)%s",
    "^(作成|生成|初期化|定義|宣言)する",
    "^(変数|引数|クラス|構造体)を",
    "^(インポート|参照|取得|設定)する",
    "^(increment|decrement|set|get|return|call|loop over|iterate over)%s",
    "^(ここで|まず|次に|そして|最後に)",
    "^step%s*%d+",
    "^(todo|fixme|xxx).*(later|後で|いつか)",
    "^(note|補足)%s*:%s*(this is|これは)",
    "^(引数|戻り値|返り値|パラメータ)%s*[:：]",
    "^(注意|警告|※)%s*[:：]?",
    "^(src|crates).*%.(rs|slint|wgsl|py)$"
}

local TRIVIAL_ECHO_RATIO = 0.85
local TRIVIAL_ECHO_MAXLEN = 40

local function get_ext(path)
    return path:match("%.[^.]+$") or ""
end

local function strip_marker(text, lc)
    local t = text:match("^%s*(.-)%s*$") or ""
    if t:sub(1, #lc) == lc then
        t = t:sub(#lc + 1):match("^%s*(.-)%s*$") or ""
    end
    return t
end

local function split_words(str)
    local words = {}
    for word in str:gmatch("[A-Za-z0-9_]+") do
        words[word] = true
    end
    return words
end

local function token_overlap(comment, code_line)
    local c = split_words(comment:lower())
    local k = split_words(code_line:lower())
    
    local c_count, intersect = 0, 0
    for w in pairs(c) do
        c_count = c_count + 1
        if k[w] then intersect = intersect + 1 end
    end
    
    if c_count == 0 then return 0.0 end
    return intersect / c_count
end

local function regex_match_insensitive(text, pattern)
    local low_text = text:lower()
    local low_pattern = pattern:lower()
    return low_text:find(low_pattern) ~= nil
end

local function classify(comment_text, next_code_line, lc)
    local body = strip_marker(comment_text, lc)
    if body == "" then return "empty" end
    
    for _, pat in ipairs(NOISE_PATTERNS) do
        if regex_match_insensitive(body, pat) then
            return "noise"
        end
    end
    
    if next_code_line and #body < TRIVIAL_ECHO_MAXLEN then
        if token_overlap(body, next_code_line) >= TRIVIAL_ECHO_RATIO then
            return "echo"
        end
    end
    return "keep"
end

local function comment_blocks(lines, lc)
    local blocks = {}
    local i = 1
    local n = #lines
    while i <= n do
        if lines[i]:match("^%s*" .. lc) then
            local start = i
            while i <= n and lines[i]:match("^%s*" .. lc) do
                i = i + 1
            end
            table.insert(blocks, {start, i - 1})
        else
            i = i + 1
        end
    end
    return blocks
end

local function scan_lines(lines, lc)
    local out = {}
    for i, line in ipairs(lines) do
        if line:match("^%s*" .. lc) then
            local next_code = nil
            for j = i + 1, #lines do
                if lines[j]:match("%S") and not lines[j]:match("^%s*" .. lc) then
                    next_code = lines[j]
                    break
                end
            end
            table.insert(out, {i, classify(line, next_code, lc)})
        end
    end
    return out
end

local function vocab_hit_blocks(lines, lc)
    local idx = {}
    local blocks = comment_blocks(lines, lc)
    for _, block in ipairs(blocks) do
        local start, fin = block[1], block[2]
        local block_text_table = {}
        for l = start, fin do
            table.insert(block_text_table, strip_marker(lines[l], lc))
        end
        local block_text = table.concat(block_text_table, "\n")
        
        for _, pat in ipairs(NOISE_PATTERNS) do
            if regex_match_insensitive(block_text, pat) then
                for l = start, fin do
                    idx[l] = true
                end
                break
            end
        end
    end
    return idx
end

local function read_file_lines(path)
    local lines = {}
    local f = io.open(path, "r")
    if not f then return nil end
    for line in f:lines() do
        table.insert(lines, line)
    end
    f:close()
    return lines
end

local function write_file_lines(path, lines)
    local f = io.open(path, "w")
    if not f then return false end
    for _, line in ipairs(lines) do
        f:write(line .. "\n")
    end
    f:close()
    return true
end

local function process(path, fix, levels)
    local ext = get_ext(path)
    local lc = LINE_COMMENT[ext]
    if not lc then return false end

    local lines = read_file_lines(path)
    if not lines then return false end

    local verdicts = scan_lines(lines, lc)
    local hit = false
    local remove_idx = {}

    for _, v in ipairs(verdicts) do
        local idx, verdict = v[1], v[2]
        if levels[verdict] then
            hit = true
            print(string.format("%s:%d: [%s] %s", path, idx, verdict, lines[idx]:match("^%s*(.-)%s*$")))
            remove_idx[idx] = true
        end
    end

    local vocab_idx = vocab_hit_blocks(lines, lc)
    for idx in pairs(vocab_idx) do
        if not remove_idx[idx] then
            hit = true
            print(string.format("%s:%d: [vocab-block] %s", path, idx, lines[idx]:match("^%s*(.-)%s*$")))
            remove_idx[idx] = true
        end
    end

    if fix and hit then
        local new_lines = {}
        for i, line in ipairs(lines) do
            if not remove_idx[i] then
                table.insert(new_lines, line)
            end
        end
        write_file_lines(path, new_lines)
    end

    return hit
end

local function main()
    local root = "."
    local fix = false
    local levels_str = "noise,echo,empty"
    local exclude_list = {"aviutl2_sdk", "target", "%.git", "slang"}

    local i = 1
    while i <= #arg do
        if arg[i] == "--fix" then
            fix = true
            i = i + 1
        elseif arg[i] == "--levels" then
            levels_str = arg[i+1]
            i = i + 2
        elseif arg[i] == "--exclude" then
            table.insert(exclude_list, arg[i+1])
            i = i + 2
        else
            root = arg[i]
            i = i + 1
        end
    end

    local levels = {}
    for lvl in levels_str:gmatch("[^,]+") do
        levels[lvl] = true
    end

    local find_cmd = "find " .. root .. " -type f"
    local p = io.popen(find_cmd)
    if not p then return end

    local n = 0
    for path in p:lines() do
        local should_exclude = false
        for _, excl in ipairs(exclude_list) do
            if path:find(excl) then
                should_exclude = true
                break
            end
        end

        if not should_exclude then
            local ext = get_ext(path)
            if LINE_COMMENT[ext] then
                if process(path, fix, levels) then
                    n = n + 1
                end
            end
        end
    end
    p:close()

    print(string.format("検出/処理ファイル数: %d", n))
end

main()