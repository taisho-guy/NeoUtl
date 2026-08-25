#!/usr/bin/env luajit

local DEFAULT_EXTENSIONS = {
    ".cpp", ".hpp", ".c", ".h", ".hh", ".hxx", ".cc", ".cxx", ".rs", ".toml",
    ".slint", ".qml", ".qrc", ".ui", ".pro", ".pri", ".js", ".jsx", ".ts",
    ".tsx", ".json", ".cmake", ".sh", ".bash", ".py", ".lua", ".fish", ".xml",
    ".yaml", ".yml", ".ini", ".conf", ".clang-format", ".clang-tidy",
    ".gitignore", ".md", ".txt", ".rst", ".glsl", ".frag", ".vert", ".slang",
    ".wgsl", ".def", ".vcxproj", ".filters", ".props", ".targets", ".sln",
    ".vcproj", ".vdproj", ".sass", ".scss", ".css", ".html", ".htm",
    ".nuspec", ".config", ".editorconfig", ".resx", ".manifest", ".metal", 
    ".vert", ".glsl", ".hlsl", "js", ".proto"
}

local DEFAULT_FILENAMES = {
    ["CMakeLists.txt"]=true, ["Makefile"]=true, ["Dockerfile"]=true,
    ["Vagrantfile"]=true, [".gitignore"]=true, ["LICENSE"]=true, ["README"]=true,
    ["nuget.config"]=true, ["packages.config"]=true, ["NuGet.Config"]=true
}

local EXCLUDE_FILES = {
    [".DS_Store"]=true, ["Thumbs.db"]=true, ["package-lock.json"]=true,
    ["yarn.lock"]=true, ["Icons.js"]=true, ["Cargo.lock"]=true
}

local EXCLUDE_DIRS = {
    "neoutl-wgpu", "Carla"
}

local IS_WINDOWS = os.getenv("OS") and os.getenv("OS"):match("[Ww]indows") or os.getenv("WINDIR") ~= nil

local function normalize_path(path)
    if not path then return "" end
    path = path:gsub("\\", "/")
    return (path:gsub("^%./", ""))
end

local function parse_path(path)
    path = normalize_path(path)
    local filename = path:match("[^/]+$") or path
    local ext = filename:match("%.[^.]+$") or ""
    return filename, ext:lower()
end

local function load_gitignore(root_dir)
    local ignore_patterns = {}
    local f = io.open(root_dir .. "/.gitignore", "r")
    if not f then return ignore_patterns end

    for line in f:lines() do
        line = line:match("^%s*(.-)%s*$")
        if line and line ~= "" and not line:match("^#") then
            line = line:gsub("/+$", "")
            table.insert(ignore_patterns, normalize_path(line))
        end
    end
    f:close()
    return ignore_patterns
end

local function is_text_file(filepath)
    local f = io.open(filepath, "rb")
    if not f then return false end
    local chunk = f:read(1024)
    f:close()
    if not chunk then return true end
    return not chunk:find("\0", 1, true)
end

local function scan_directory(root_dir, ignore_patterns)
    local files = {}
    local cmd

    if IS_WINDOWS then
        cmd = 'dir /b /s /a-d "' .. root_dir .. '" 2>nul'
    else
        cmd = 'find "' .. root_dir .. '" -type f'
    end

    local p = io.popen(cmd)
    if not p then return files end

    local current_dir = ""
    if IS_WINDOWS then
        local pwd_p = io.popen("cd")
        if pwd_p then
            current_dir = normalize_path(pwd_p:read("*l") or "")
            pwd_p:close()
        end
    end

    for raw_path in p:lines() do
        local path = normalize_path(raw_path)
        
        if current_dir ~= "" and path:sub(1, #current_dir) == current_dir then
            path = path:sub(#current_dir + 2)
        end

        local should_exclude = false
        local filename, _ = parse_path(path)

        if path:find("/%.git/") or path:match("/%.git$") then
            should_exclude = true
        end

        do
            local rel_path = path
            for _, target in ipairs(EXCLUDE_DIRS) do
                target = target:gsub("^/+", ""):gsub("/+$", "")
                if rel_path == target or rel_path:sub(1, #target + 1) == target .. "/" then
                    should_exclude = true
                    break
                end
            end
        end

        if filename:find("project_context") then
            should_exclude = true
        end

        if EXCLUDE_FILES[filename] then
            should_exclude = true
        end

        if not should_exclude then
            local rel_path = path
            for _, pattern in ipairs(ignore_patterns) do
                local anchored = pattern:match("^/(.+)$")
                local target = anchored or pattern
                local match = false
                if anchored then
                    match = rel_path == target or rel_path:sub(1, #target + 1) == target .. "/"
                else
                    for _, segment in ipairs((function()
                        local segs = {}
                        for s in rel_path:gmatch("[^/]+") do table.insert(segs, s) end
                        return segs
                    end)()) do
                        if segment == target then match = true break end
                    end
                    if rel_path == target or rel_path:sub(1, #target + 1) == target .. "/" then
                        match = true
                    end
                end
                if match then
                    should_exclude = true
                    break
                end
            end
        end

        if not should_exclude then
            table.insert(files, path)
        end
    end
    p:close()
    table.sort(files)
    return files
end

local function generate_tree(root_dir, files)
    local tree_lines = {}
    local root_name = root_dir
    if root_dir == "." then
        local pwd = normalize_path(os.getenv("CD") or os.getenv("PWD") or ".")
        root_name = pwd:match("[^/]+$") or pwd
    end
    table.insert(tree_lines, root_name .. "/")

    for _, filepath in ipairs(files) do
        local rel_path = filepath
        local filename, _ = parse_path(filepath)

        local is_hidden = filename:match("^%.") and filename ~= ".gitignore"
        local is_context = filename:find("project_context")

        if not is_hidden and not is_context then
            local parts = {}
            for part in rel_path:gmatch("[^/]+") do table.insert(parts, part) end
            
            local level = #parts - 1
            local indent = string.rep("  ", level)
            local prefix = level > 0 and "|-- " or "+-- "
            table.insert(tree_lines, indent .. prefix .. parts[#parts])
        end
    end
    return table.concat(tree_lines, "\n")
end

local function should_process(filepath, output_file, allowed_exts)
    local filename, ext = parse_path(filepath)
    
    if filepath == output_file then return false end
    if filename:find("project_context") then return false end
    if EXCLUDE_FILES[filename] then return false end
    if DEFAULT_FILENAMES[filename] then return true end
    
    if allowed_exts[ext] then 
        return is_text_file(filepath) 
    end
    return false
end

local function escape_xml(str)
    return str:gsub("&", "&amp;"):gsub("<", "&lt;"):gsub(">", "&gt;"):gsub('"', "&quot;"):gsub("'", "&apos;")
end

local function escape_cdata(content)
    return content:gsub("]]>", "]]]]><![CDATA[>")
end

local function main()
    local output_file = nil
    local root_dir = "."
    
    local allowed_exts = {}
    for _, ext in ipairs(DEFAULT_EXTENSIONS) do allowed_exts[ext] = true end

    local i = 1
    while i <= #arg do
        if arg[i] == "-o" or arg[i] == "--output" then
            output_file = arg[i+1]
            i = i + 2
        elseif arg[i] == "-d" or arg[i] == "--dir" then
            root_dir = arg[i+1]
            i = i + 2
        elseif arg[i] == "-e" or arg[i] == "--ext" then
            local custom_ext = arg[i+1]
            if custom_ext then
                if not custom_ext:match("^%.") then custom_ext = "." .. custom_ext end
                allowed_exts[custom_ext:lower()] = true
            end
            i = i + 2
        elseif arg[i] == "-h" or arg[i] == "--help" then
            print("Usage: luajit export.lua [-o OUTPUT_FILE] [-d ROOT_DIR] [-e ADDITIONAL_EXT]")
            os.exit(0)
        else
            i = i + 1
        end
    end

    if not output_file then
        output_file = "project_context_" .. os.date("%Y%m%d_%H%M%S") .. ".xml"
    end

    root_dir = root_dir:gsub("/+$", "")

    print("[*] Loading .gitignore patterns...")
    local ignore_patterns = load_gitignore(root_dir)

    print("[*] Scanning structure...")
    local all_files = scan_directory(root_dir, ignore_patterns)
    local tree_structure = generate_tree(root_dir, all_files)

    local out_f = io.open(output_file, "w")
    if not out_f then
        print("[!] Error writing output file: " .. output_file)
        os.exit(1)
    end

    local project_name = root_dir:match("[^/]+$") or root_dir
    local timestamp = os.date("%Y-%m-%d %H:%M:%S")

    out_f:write('<?xml version="1.0" encoding="UTF-8"?>\n')
    out_f:write('<project name="' .. escape_xml(project_name) .. '">\n')
    out_f:write('  <metadata>\n')
    out_f:write('    <generated_date>' .. escape_xml(timestamp) .. '</generated_date>\n')
    out_f:write('    <generated_by>export.lua (XML Version)</generated_by>\n')
    out_f:write('    <structure><![CDATA[\n' .. tree_structure .. '\n]]></structure>\n')
    out_f:write('  </metadata>\n')
    out_f:write('  <files>\n')

    print("[*] Reading and embedding files into XML...")
    local total_files = 0
    local extension_counts = {}
    local my_filename = parse_path(arg[0] or "")

    for _, filepath in ipairs(all_files) do
        local filename, ext = parse_path(filepath)
        
        if filename ~= my_filename and filename ~= "export.py" and filename ~= "export.lua" then
            if should_process(filepath, output_file, allowed_exts) then
                local f = io.open(filepath, "r")
                if f then
                    local content = f:read("*a")
                    f:close()

                    local rel_path = filepath
                    extension_counts[ext] = (extension_counts[ext] or 0) + 1
                    total_files = total_files + 1

                    out_f:write('    <file path="' .. escape_xml(rel_path) .. '" bytes="' .. #content .. '" extension="' .. escape_xml(ext) .. '">\n')
                    out_f:write('      <content><![CDATA[' .. escape_cdata(content) .. ']]></content>\n')
                    out_f:write('    </file>\n')
                else
                    print("[!] Skipping " .. filepath .. ": Cannot open file")
                end
            end
        end
    end

    out_f:write('  </files>\n')
    out_f:write('  <summary>\n')
    out_f:write('    <total_processed_files>' .. total_files .. '</total_processed_files>\n')
    out_f:write('    <file_counts>\n')
    for k, v in pairs(extension_counts) do
        local tag_name = k == "" and "no_ext" or k:sub(2)
        tag_name = tag_name:gsub("[^%a%d_]", "_")
        out_f:write('      <' .. tag_name .. '>' .. v .. '</' .. tag_name .. '>\n')
    end
    out_f:write('    </file_counts>\n')
    out_f:write('  </summary>\n')
    out_f:write('</project>\n')

    out_f:close()
    print("[+] XML export completed: " .. output_file)
end

main()