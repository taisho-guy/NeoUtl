local script_dir = debug.getinfo(1, "S").source:match('@(.+)')
script_dir = script_dir and script_dir:match('^(.*)/[^/]+$') or "."

local function shell_quote(s)
    return "'" .. s:gsub("'", "'\"'\"'") .. "'"
end

local function path_join(a, b)
    if a:sub(-1) == "/" then
        return a .. b
    end
    return a .. "/" .. b
end

local function exists(path)
    local ok = os.rename(path, path)
    if ok then return true end
    return false
end

local function read_file(path)
    local file = assert(io.open(path, "rb"), "failed to open: " .. path)
    local data = file:read("*a")
    file:close()
    return data
end

local function write_file(path, data)
    local dir = path:match("(.+)/[^/]+$")
    if dir and not exists(dir) then
        os.execute("mkdir -p " .. shell_quote(dir))
    end

    local file = assert(io.open(path, "wb"), "failed to write: " .. path)
    file:write(data)
    file:close()
end

local function recursive_find_files(dir)
    local list = {}
    local pipe = io.popen("find " .. shell_quote(dir) .. " -type f")
    if not pipe then
        return list
    end

    for path in pipe:lines() do
        list[#list + 1] = path
    end
    pipe:close()
    return list
end

local function find_version_in_file(path)
    local data = read_file(path)
    local version = data:match('version%s*=%s*"([^"]+)"')
    if version then
        return version
    end
    return data:match('v([0-9]+%.[0-9]+%.[0-9]+)')
end

local function escape_pattern(s)
    return s:gsub('([%(%)%.%%%+%-%*%?%[%]%^%$])', '%%%1')
end

local function replace_version_in_file(path, current_version, target_version)
    local data = read_file(path)
    local plain = data:find(current_version, 1, true)
    if not plain then
        return false
    end

    local pattern = escape_pattern(current_version)
    local updated = data:gsub(pattern, target_version)
    if updated ~= data then
        write_file(path, updated)
        return true
    end

    return false
end

local function copy_dir(src, dst)
    os.execute("mkdir -p " .. shell_quote(dst))
    os.execute("cp -a " .. shell_quote(src) .. "/. " .. shell_quote(dst) .. "/")
end

local function normalize_version(arg)
    local cleaned = arg:gsub('^%-%-?', '')
    cleaned = cleaned:gsub('^v', '')
    return cleaned
end

local args = {}
for _, arg in ipairs(arg) do
    args[#args + 1] = arg
end

local target_version = nil
for _, arg in ipairs(args) do
    if arg == "-h" or arg == "--help" then
        print("usage: luajit update.lua --0.8.0")
        print("       luajit update.lua 0.8.0")
        os.exit(0)
    end

    if arg:match('^%-') then
        target_version = normalize_version(arg)
    elseif not target_version and arg:match('^[0-9]+%.[0-9]+%.[0-9]+$') then
        target_version = arg
    end
end

if not target_version then
    io.stderr:write("version is required, e.g. luajit update.lua --0.8.0\n")
    os.exit(1)
end

local root_dir = script_dir
local cargo_path = path_join(root_dir, "Cargo.toml")
local current_version = find_version_in_file(cargo_path)
if not current_version then
    io.stderr:write("failed to detect current version from Cargo.toml\n")
    os.exit(1)
end

local source_dir = path_join(path_join(root_dir, "sdk/neoutl"), current_version)
local target_dir = path_join(path_join(root_dir, "sdk/neoutl"), target_version)

if not exists(source_dir) then
    io.stderr:write("source SDK directory does not exist: " .. source_dir .. "\n")
    os.exit(1)
end

if exists(target_dir) then
    io.stderr:write("target SDK directory already exists: " .. target_dir .. "\n")
    io.stderr:write("remove it first or choose another version\n")
    os.exit(1)
end

print("1/4 current version: " .. current_version)
print("2/4 copying SDK: " .. source_dir .. " -> " .. target_dir)
copy_dir(source_dir, target_dir)

print("3/4 replacing version strings in Cargo.toml and SDK files")
local files = { cargo_path }
for _, file in ipairs(recursive_find_files(target_dir)) do
    files[#files + 1] = file
end

for _, file in ipairs(files) do
    if file:match("%.lua$") then
            else
        replace_version_in_file(file, current_version, target_version)
    end
end

print("4/4 done")
print("updated SDK path: sdk/neoutl/" .. target_version)
