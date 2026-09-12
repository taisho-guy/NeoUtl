#!/usr/bin/env luajit

local json = nil
pcall(function() json = require("cjson") end)
if not json then
    pcall(function() json = require("dkjson") end)
end

local escape_map = {
    ['"']  = '\\"',
    ['\\'] = '\\\\',
    ['\b'] = '\\b',
    ['\f'] = '\\f',
    ['\n'] = '\\n',
    ['\r'] = '\\r',
    ['\t'] = '\\t'
}

local function encode_string(s)
    return '"' .. s:gsub('[%c"\\]', function(c)
        if escape_map[c] then
            return escape_map[c]
        else
            return string.format("\\u%04x", string.byte(c))
        end
    end) .. '"'
end

local function json_encode_fallback(val)
    local t = type(val)
    if t == "nil" then return "null"
    elseif t == "boolean" then return tostring(val)
    elseif t == "number" then 
        if val ~= val or val == math.huge or val == -math.huge then
            return "null"
        end
        return tostring(val)
    elseif t == "string" then
        return encode_string(val)
    elseif t == "table" then
        local is_array = true
        local max_i = 0
        for k, _ in pairs(val) do
            if type(k) ~= "number" or k < 1 or math.floor(k) ~= k then
                is_array = false
                break
            end
            if k > max_i then max_i = k end
        end
        if is_array then
            local parts = {}
            for i = 1, max_i do
                table.insert(parts, json_encode_fallback(val[i]))
            end
            return "[" .. table.concat(parts, ",") .. "]"
        else
            local parts = {}
            for k, v in pairs(val) do
                table.insert(parts, encode_string(tostring(k)) .. ":" .. json_encode_fallback(v))
            end
            return "{" .. table.concat(parts, ",") .. "}"
        end
    end
    return "null"
end

local function encode_json(val)
    if json and json.encode then
        return json.encode(val)
    else
        return json_encode_fallback(val)
    end
end

local DEFAULT_EXTENSIONS = {
    ".c", ".h", ".cpp", ".hpp", ".cc", ".hh", ".cxx", ".hxx", ".def", ".inl", ".ipp", ".tcc",
    ".cs", ".csx",
    ".vcxproj", ".vcxproj.user", ".filters", ".props", ".targets", ".sln", ".vcproj", ".vdproj",
    ".cmake", ".make", ".mk", ".gradle", ".groovy", ".bazel", ".bzl", ".dockerfile",
    ".tf", ".tfvars", ".nix", ".m4", ".in", ".pkr.hcl", ".pkr.json",
    ".py", ".pyw", ".lua", ".sh", ".bash", ".zsh", ".fish", ".ps1", ".psm1", ".psd1", ".bat", ".cmd",
    ".rb", ".rake", ".pl", ".pm", ".t", ".php", ".phtml",
    ".rs", ".go", ".swift", ".kt", ".kts", ".java", ".scala", ".sc", ".clj", ".cljs", ".edn",
    ".zig", ".nim", ".cr", ".d", ".di", ".dart", ".elm", ".ex", ".exs", ".erl", ".hrl",
    ".hs", ".lhs", ".ocaml", ".ml", ".mli", ".fs", ".fsi", ".fsx", ".v", ".sv", ".vhdl",
    ".js", ".jsx", ".mjs", ".cjs", ".ts", ".tsx", ".mts", ".cts",
    ".html", ".htm", ".xhtml", ".vue", ".svelte", ".astro",
    ".css", ".sass", ".scss", ".less", ".styl", ".wasm", ".wat",
    ".slint", ".qml", ".qrc", ".ui", ".pro", ".pri", ".xib", ".storyboard",
    ".glsl", ".frag", ".vert", ".geom", ".comp", ".tesc", ".tese", ".slang", ".wgsl", ".metal", ".hlsl", ".shader",
    ".json", ".json5", ".jsonc", ".ini", ".conf", ".config", ".xml", ".yaml", ".yml", ".toml", ".csv", ".tsv",
    ".resx", ".nuspec", ".editorconfig", ".manifest", ".clang-format", ".clang-tidy", ".prettierrc",
    ".eslintrc", ".babelrc", ".stylelintrc", ".env", ".env.example", ".properties",
    ".proto", ".graphql", ".gql", ".sql", ".prisma", ".surrealql",
    ".gitignore", ".gitattributes", ".gitmodules", ".md", ".markdown", ".mdx", ".txt", ".rst", ".adoc",
    ".tex", ".bib", ".org", ".lic", ".license"
}

local DEFAULT_FILENAMES = {
    ["CMakeLists.txt"]=true, ["Makefile"]=true, ["Dockerfile"]=true, ["docker-compose.yml"]=true, ["docker-compose.yaml"]=true,
    ["Vagrantfile"]=true, [".gitignore"]=true, [".gitattributes"]=true, [".gitmodules"]=true, [".editorconfig"]=true,
    ["LICENSE"]=true, ["LICENSE.md"]=true, ["LICENSE.txt"]=true, ["README"]=true, ["README.md"]=true, ["README.txt"]=true,
    ["nuget.config"]=true, ["packages.config"]=true, ["NuGet.Config"]=true, ["BUILD"]=true, ["WORKSPACE"]=true,
    ["Procfile"]=true, ["Gemfile"]=true, ["Rakefile"]=true, ["Containerfile"]=true, ["dune"]=true, ["dune-project"]=true
}

local EXCLUDE_FILES = {
    [".DS_Store"]=true, ["Thumbs.db"]=true, ["package-lock.json"]=true,
    ["yarn.lock"]=true, ["pnpm-lock.yaml"]=true, ["bun.lockb"]=true,
    ["Cargo.lock"]=true, ["poetry.lock"]=true, ["Pipfile.lock"]=true,
    ["mix.lock"]=true, ["composer.lock"]=true, ["Icons.js"]=true
}

local EXCLUDE_DIRS = {
    "neoutl-wgpu", "Carla", "node_modules", ".git", ".svn", ".hg", "target", "build", "dist",
    "out", ".next", ".nuxt", "__pycache__", ".venv", "venv", ".idea", ".vscode"
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
                if rel_path == target or rel_path:sub(1, #target + 1) == target .. "/" or rel_path:find("/" .. target .. "/") then
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

        local is_hidden = filename:match("^%.") and not DEFAULT_FILENAMES[filename]
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
        output_file = "project_context_" .. os.date("%Y%m%d_%H%M%S") .. ".json"
    end

    root_dir = root_dir:gsub("/+$", "")

    print("[*] Loading .gitignore patterns...")
    local ignore_patterns = load_gitignore(root_dir)

    print("[*] Scanning structure...")
    local all_files = scan_directory(root_dir, ignore_patterns)
    local tree_structure = generate_tree(root_dir, all_files)

    local project_name = root_dir:match("[^/]+$") or root_dir
    local timestamp = os.date("%Y-%m-%d %H:%M:%S")

    local exported_data = {
        project = {
            name = project_name,
            metadata = {
                generated_date = timestamp,
                generated_by = "export.lua (UTF-8 Safe Version)",
                structure = tree_structure
            },
            files = {},
            summary = {
                total_processed_files = 0,
                file_counts = {}
            }
        }
    }

    print("[*] Reading and embedding files into JSON structure...")
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
                    local ext_key = ext == "" and "no_ext" or ext
                    extension_counts[ext_key] = (extension_counts[ext_key] or 0) + 1
                    total_files = total_files + 1

                    table.insert(exported_data.project.files, {
                        path = rel_path,
                        bytes = #content,
                        extension = ext,
                        content = content
                    })
                else
                    print("[!] Skipping " .. filepath .. ": Cannot open file")
                end
            end
        end
    end

    exported_data.project.summary.total_processed_files = total_files
    exported_data.project.summary.file_counts = extension_counts

    local out_f = io.open(output_file, "w")
    if not out_f then
        print("[!] Error writing output file: " .. output_file)
        os.exit(1)
    end

    out_f:write(encode_json(exported_data))
    out_f:close()
    print("[+] JSON export completed: " .. output_file)
end

main()