//! The task suite of the projection trial (PX-114): thirty small coding
//! tasks, each a committed repository whose mandatory check fails until the
//! task is done and passes once it is.
//!
//! Every task ships its reference solution. The suite is valid only when
//! each task's check fails on the starting repository and passes with the
//! reference applied (`tests/projection_trial.rs`), so a trial's accuracy
//! measures the model and the surface, not a broken task. The tasks are
//! deliberately plain Python so the check needs only `python3`.

/// One task of the suite.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrialTask {
    /// Stable id, the same string in every arm of a pair.
    pub id: String,
    /// What the agent is asked, as a person would write it.
    pub goal: String,
    /// The starting repository: path and content.
    pub files: Vec<(String, String)>,
    /// The reference solution: the files that change, whole.
    pub reference: Vec<(String, String)>,
}

/// The mandatory check every task repository declares.
pub const CHECK_ARGV: [&str; 2] = ["python3", "check.py"];

fn task(id: &str, goal: &str, files: &[(&str, &str)], reference: &[(&str, &str)]) -> TrialTask {
    let mut files: Vec<(String, String)> = files
        .iter()
        .map(|(p, c)| ((*p).to_owned(), (*c).to_owned()))
        .collect();
    files.push((
        ".modbit/verification.json".into(),
        "{\"commands\": [{\"id\": \"unit\", \"argv\": [\"python3\", \"check.py\"]}]}".into(),
    ));
    TrialTask {
        id: id.to_owned(),
        goal: goal.to_owned(),
        files,
        reference: reference
            .iter()
            .map(|(p, c)| ((*p).to_owned(), (*c).to_owned()))
            .collect(),
    }
}

/// F1: an off-by-one at the end of a range.
fn off_by_one() -> Vec<TrialTask> {
    vec![
        task(
            "range-sum-inclusive",
            "range_sum(a, b) in rangesum.py should add every integer from a to b inclusive, but it stops one short. Fix it so check.py passes.",
            &[
                (
                    "rangesum.py",
                    "def range_sum(a, b):\n    return sum(range(a, b))\n",
                ),
                (
                    "check.py",
                    "from rangesum import range_sum\nassert range_sum(1, 4) == 10\nassert range_sum(5, 5) == 5\nassert range_sum(-2, 2) == 0\n",
                ),
            ],
            &[(
                "rangesum.py",
                "def range_sum(a, b):\n    return sum(range(a, b + 1))\n",
            )],
        ),
        task(
            "count-multiples-inclusive",
            "count_multiples(lo, hi, k) in multiples.py counts the multiples of k between lo and hi inclusive, but it misses hi when hi is itself a multiple. Fix it so check.py passes.",
            &[
                (
                    "multiples.py",
                    "def count_multiples(lo, hi, k):\n    return len([n for n in range(lo, hi) if n % k == 0])\n",
                ),
                (
                    "check.py",
                    "from multiples import count_multiples\nassert count_multiples(1, 10, 5) == 2\nassert count_multiples(3, 3, 3) == 1\nassert count_multiples(1, 9, 5) == 1\n",
                ),
            ],
            &[(
                "multiples.py",
                "def count_multiples(lo, hi, k):\n    return len([n for n in range(lo, hi + 1) if n % k == 0])\n",
            )],
        ),
        task(
            "last-n-items",
            "last_n(items, n) in tail.py should return the final n items (all of them when n exceeds the length, and an empty list for n == 0). It is wrong for n == 0 and drops one item otherwise. Fix it so check.py passes.",
            &[
                (
                    "tail.py",
                    "def last_n(items, n):\n    return items[-n - 1:]\n",
                ),
                (
                    "check.py",
                    "from tail import last_n\nassert last_n([1, 2, 3, 4], 2) == [3, 4]\nassert last_n([1, 2], 5) == [1, 2]\nassert last_n([1, 2], 0) == []\n",
                ),
            ],
            &[(
                "tail.py",
                "def last_n(items, n):\n    if n <= 0:\n        return []\n    return items[-n:]\n",
            )],
        ),
    ]
}

/// F2: a stub to implement.
fn stubs() -> Vec<TrialTask> {
    vec![
        task(
            "implement-slugify",
            "Implement slugify(text) in slug.py: lower-case, replace every run of characters that are not letters or digits with a single hyphen, and trim hyphens from both ends. check.py shows the expected behaviour.",
            &[
                (
                    "slug.py",
                    "def slugify(text):\n    raise NotImplementedError\n",
                ),
                (
                    "check.py",
                    "from slug import slugify\nassert slugify('Hello, World!') == 'hello-world'\nassert slugify('  a  b  ') == 'a-b'\nassert slugify('---x---') == 'x'\nassert slugify('R2-D2 & C-3PO') == 'r2-d2-c-3po'\n",
                ),
            ],
            &[(
                "slug.py",
                "import re\n\n\ndef slugify(text):\n    return re.sub(r'[^a-z0-9]+', '-', text.lower()).strip('-')\n",
            )],
        ),
        task(
            "implement-snake-case",
            "Implement to_snake(name) in casing.py: convert CamelCase or camelCase to snake_case, keeping runs of capitals together (HTTPServer becomes http_server). check.py shows the expected behaviour.",
            &[
                (
                    "casing.py",
                    "def to_snake(name):\n    raise NotImplementedError\n",
                ),
                (
                    "check.py",
                    "from casing import to_snake\nassert to_snake('camelCase') == 'camel_case'\nassert to_snake('CamelCase') == 'camel_case'\nassert to_snake('HTTPServer') == 'http_server'\nassert to_snake('already_snake') == 'already_snake'\nassert to_snake('parseXMLFile') == 'parse_xml_file'\n",
                ),
            ],
            &[(
                "casing.py",
                "import re\n\n\ndef to_snake(name):\n    s = re.sub(r'(.)([A-Z][a-z]+)', r'\\1_\\2', name)\n    return re.sub(r'([a-z0-9])([A-Z])', r'\\1_\\2', s).lower()\n",
            )],
        ),
        task(
            "implement-truncate-words",
            "Implement truncate_words(text, limit) in words.py: keep whole words while the result stays within limit characters, and append '...' when anything was cut (the ellipsis counts toward the limit). check.py shows the expected behaviour.",
            &[
                (
                    "words.py",
                    "def truncate_words(text, limit):\n    raise NotImplementedError\n",
                ),
                (
                    "check.py",
                    "from words import truncate_words\nassert truncate_words('hello world', 20) == 'hello world'\nassert truncate_words('the quick brown fox', 12) == 'the quick...'\nassert truncate_words('abcdefghij', 5) == '...'\nassert len(truncate_words('one two three four five', 15)) <= 15\n",
                ),
            ],
            &[(
                "words.py",
                "def truncate_words(text, limit):\n    if len(text) <= limit:\n        return text\n    room = limit - 3\n    out = ''\n    for word in text.split(' '):\n        candidate = word if not out else out + ' ' + word\n        if len(candidate) > room:\n            break\n        out = candidate\n    return out + '...'\n",
            )],
        ),
    ]
}

/// F3: a rename that touches the definition and two callers.
fn renames() -> Vec<TrialTask> {
    let one = |id: &str, old: &str, new: &str, module: &str| {
        let def = format!("def {old}(key):\n    return {{'id': key}}\n");
        let def_new = format!("def {new}(key):\n    return {{'id': key}}\n");
        let a = format!(
            "from {module} import {old}\n\n\ndef show(key):\n    return str({old}(key)['id'])\n"
        );
        let a_new = format!(
            "from {module} import {new}\n\n\ndef show(key):\n    return str({new}(key)['id'])\n"
        );
        let b = format!(
            "from {module} import {old}\n\n\ndef exists(key):\n    return {old}(key) is not None\n"
        );
        let b_new = format!(
            "from {module} import {new}\n\n\ndef exists(key):\n    return {new}(key) is not None\n"
        );
        let check = format!(
            "import inspect\nimport {module}\nimport view_a\nimport view_b\nassert hasattr({module}, '{new}')\nassert not hasattr({module}, '{old}')\nassert view_a.show(7) == '7'\nassert view_b.exists(3) is True\nassert '{old}' not in inspect.getsource(view_a) + inspect.getsource(view_b)\n"
        );
        let module_file = format!("{module}.py");
        task(
            id,
            &format!(
                "Rename {old} to {new} everywhere: the definition in {module_file} and its two callers in view_a.py and view_b.py. Nothing should still use the old name; check.py verifies it."
            ),
            &[
                (module_file.as_str(), def.as_str()),
                ("view_a.py", a.as_str()),
                ("view_b.py", b.as_str()),
                ("check.py", check.as_str()),
            ],
            &[
                (module_file.as_str(), def_new.as_str()),
                ("view_a.py", a_new.as_str()),
                ("view_b.py", b_new.as_str()),
            ],
        )
    };
    vec![
        one("rename-get-user", "get_user", "fetch_user", "users"),
        one("rename-load-order", "load_order", "read_order", "orders"),
        one("rename-find-item", "find_item", "lookup_item", "catalog"),
    ]
}

/// F4: input validation.
fn validation() -> Vec<TrialTask> {
    vec![
        task(
            "validate-port",
            "parse_port(text) in ports.py should return the port as an int, and raise ValueError for anything that is not an integer from 1 to 65535. Today it accepts out-of-range numbers. Fix it so check.py passes.",
            &[
                ("ports.py", "def parse_port(text):\n    return int(text)\n"),
                (
                    "check.py",
                    "from ports import parse_port\nassert parse_port('8080') == 8080\nfor bad in ['0', '65536', '-1', 'abc', '']:\n    try:\n        parse_port(bad)\n    except ValueError:\n        continue\n    raise AssertionError(bad)\n",
                ),
            ],
            &[(
                "ports.py",
                "def parse_port(text):\n    port = int(text)\n    if not 1 <= port <= 65535:\n        raise ValueError(text)\n    return port\n",
            )],
        ),
        task(
            "validate-percent",
            "parse_percent(text) in percent.py should turn '42%' or '42' into 0.42 and raise ValueError for anything outside 0 to 100 or not a number. Today it returns the wrong scale and accepts out-of-range values. Fix it so check.py passes.",
            &[
                (
                    "percent.py",
                    "def parse_percent(text):\n    return float(text.rstrip('%'))\n",
                ),
                (
                    "check.py",
                    "from percent import parse_percent\nassert parse_percent('42%') == 0.42\nassert parse_percent('100') == 1.0\nassert parse_percent('0%') == 0.0\nfor bad in ['101', '-1%', 'x%', '']:\n    try:\n        parse_percent(bad)\n    except ValueError:\n        continue\n    raise AssertionError(bad)\n",
                ),
            ],
            &[(
                "percent.py",
                "def parse_percent(text):\n    value = float(text.strip().rstrip('%'))\n    if not 0 <= value <= 100:\n        raise ValueError(text)\n    return value / 100\n",
            )],
        ),
        task(
            "validate-age",
            "make_person(name, age) in person.py should reject an empty name and an age that is not an int between 0 and 150 with ValueError, and otherwise return a dict with name and age. Today it accepts anything. Fix it so check.py passes.",
            &[
                (
                    "person.py",
                    "def make_person(name, age):\n    return {'name': name, 'age': age}\n",
                ),
                (
                    "check.py",
                    "from person import make_person\nassert make_person('Ada', 36) == {'name': 'Ada', 'age': 36}\nfor name, age in [('', 3), ('Bob', -1), ('Bob', 151), ('Bob', 'x'), ('Bob', 2.5)]:\n    try:\n        make_person(name, age)\n    except ValueError:\n        continue\n    raise AssertionError((name, age))\n",
                ),
            ],
            &[(
                "person.py",
                "def make_person(name, age):\n    if not name:\n        raise ValueError('name')\n    if not isinstance(age, int) or isinstance(age, bool) or not 0 <= age <= 150:\n        raise ValueError('age')\n    return {'name': name, 'age': age}\n",
            )],
        ),
    ]
}

/// F5: the mutable default argument.
fn mutable_defaults() -> Vec<TrialTask> {
    let one = |id: &str, func: &str, arg: &str| {
        let src = format!("def {func}(x, {arg}=[]):\n    {arg}.append(x)\n    return {arg}\n");
        let fixed = format!(
            "def {func}(x, {arg}=None):\n    if {arg} is None:\n        {arg} = []\n    {arg}.append(x)\n    return {arg}\n"
        );
        let check = format!(
            "from shared import {func}\nassert {func}(1) == [1]\nassert {func}(2) == [2]\nbag = [9]\nassert {func}(3, bag) == [9, 3]\nassert bag == [9, 3]\nassert {func}(4) == [4]\n"
        );
        task(
            id,
            &format!(
                "{func}(x, {arg}) in shared.py leaks state between calls: calling it twice without the second argument accumulates items. Fix it so each call without {arg} starts from an empty list, and an {arg} that is passed is still appended to in place."
            ),
            &[("shared.py", src.as_str()), ("check.py", check.as_str())],
            &[("shared.py", fixed.as_str())],
        )
    };
    vec![
        one("mutable-default-append-item", "append_item", "items"),
        one("mutable-default-add-tag", "add_tag", "tags"),
        one("mutable-default-collect", "collect", "bucket"),
    ]
}

/// F6: a wrong key or constant between a config file and the code reading it.
fn configuration() -> Vec<TrialTask> {
    vec![
        task(
            "config-tax-rate-key",
            "total_with_tax(amount) in billing.py reads its rate from config.json but returns the amount untaxed. Find why and fix it so check.py passes (do not hard-code the rate).",
            &[
                ("config.json", "{\"tax_rate\": 0.2}\n"),
                (
                    "billing.py",
                    "import json\nimport os\n\n\ndef total_with_tax(amount):\n    with open(os.path.join(os.path.dirname(__file__), 'config.json')) as f:\n        cfg = json.load(f)\n    return round(amount * (1 + cfg.get('taxRate', 0)), 2)\n",
                ),
                (
                    "check.py",
                    "import json\nfrom billing import total_with_tax\nassert total_with_tax(100) == 120.0\nassert json.load(open('config.json'))['tax_rate'] == 0.2\n",
                ),
            ],
            &[(
                "billing.py",
                "import json\nimport os\n\n\ndef total_with_tax(amount):\n    with open(os.path.join(os.path.dirname(__file__), 'config.json')) as f:\n        cfg = json.load(f)\n    return round(amount * (1 + cfg.get('tax_rate', 0)), 2)\n",
            )],
        ),
        task(
            "config-retries-count",
            "attempts() in retry.py should report how many times a call is tried in total: the first try plus the retries from config.json. It is one short. Fix it so check.py passes.",
            &[
                ("config.json", "{\"retries\": 3}\n"),
                (
                    "retry.py",
                    "import json\nimport os\n\n\ndef attempts():\n    with open(os.path.join(os.path.dirname(__file__), 'config.json')) as f:\n        return json.load(f)['retries']\n",
                ),
                (
                    "check.py",
                    "from retry import attempts\nassert attempts() == 4\n",
                ),
            ],
            &[(
                "retry.py",
                "import json\nimport os\n\n\ndef attempts():\n    with open(os.path.join(os.path.dirname(__file__), 'config.json')) as f:\n        return json.load(f)['retries'] + 1\n",
            )],
        ),
        task(
            "config-timeout-units",
            "timeout_seconds() in net.py should return the timeout in seconds, but config.json stores milliseconds under timeout_ms and the function returns the raw number. Fix it so check.py passes.",
            &[
                ("config.json", "{\"timeout_ms\": 2500}\n"),
                (
                    "net.py",
                    "import json\nimport os\n\n\ndef timeout_seconds():\n    with open(os.path.join(os.path.dirname(__file__), 'config.json')) as f:\n        return json.load(f)['timeout_ms']\n",
                ),
                (
                    "check.py",
                    "from net import timeout_seconds\nassert timeout_seconds() == 2.5\n",
                ),
            ],
            &[(
                "net.py",
                "import json\nimport os\n\n\ndef timeout_seconds():\n    with open(os.path.join(os.path.dirname(__file__), 'config.json')) as f:\n        return json.load(f)['timeout_ms'] / 1000\n",
            )],
        ),
    ]
}

/// F7: ordering bugs.
fn ordering() -> Vec<TrialTask> {
    vec![
        task(
            "sort-top-scores",
            "top_scores(rows, n) in scores.py should return the n highest scores, best first. It returns the lowest. Fix it so check.py passes.",
            &[
                (
                    "scores.py",
                    "def top_scores(rows, n):\n    return sorted(r['score'] for r in rows)[:n]\n",
                ),
                (
                    "check.py",
                    "from scores import top_scores\nrows = [{'score': s} for s in [5, 9, 1, 7]]\nassert top_scores(rows, 2) == [9, 7]\nassert top_scores(rows, 10) == [9, 7, 5, 1]\n",
                ),
            ],
            &[(
                "scores.py",
                "def top_scores(rows, n):\n    return sorted((r['score'] for r in rows), reverse=True)[:n]\n",
            )],
        ),
        task(
            "sort-oldest-first",
            "oldest_first(people) in people.py should order dicts by their 'born' year ascending, and keep the original order for equal years. It sorts by name. Fix it so check.py passes.",
            &[
                (
                    "people.py",
                    "def oldest_first(people):\n    return sorted(people, key=lambda p: p['name'])\n",
                ),
                (
                    "check.py",
                    "from people import oldest_first\nrows = [{'name': 'c', 'born': 1990}, {'name': 'a', 'born': 1985}, {'name': 'b', 'born': 1990}]\nassert [p['name'] for p in oldest_first(rows)] == ['a', 'c', 'b']\n",
                ),
            ],
            &[(
                "people.py",
                "def oldest_first(people):\n    return sorted(people, key=lambda p: p['born'])\n",
            )],
        ),
        task(
            "sort-longest-names",
            "longest_names(names, n) in names.py should return the n longest names, longest first, ties broken alphabetically. It ignores the ties rule and the direction. Fix it so check.py passes.",
            &[
                (
                    "names.py",
                    "def longest_names(names, n):\n    return sorted(names, key=len)[:n]\n",
                ),
                (
                    "check.py",
                    "from names import longest_names\nassert longest_names(['bob', 'alice', 'eve', 'carol', 'dan'], 3) == ['alice', 'carol', 'bob']\nassert longest_names(['a'], 5) == ['a']\n",
                ),
            ],
            &[(
                "names.py",
                "def longest_names(names, n):\n    return sorted(names, key=lambda s: (-len(s), s))[:n]\n",
            )],
        ),
    ]
}

/// F8: small parsers.
fn parsers() -> Vec<TrialTask> {
    vec![
        task(
            "parse-duration",
            "Implement parse_duration(text) in durations.py: '1h30m', '45s', '2h', '90m' and '1h1m1s' give the total number of seconds as an int; anything else raises ValueError. check.py shows the cases.",
            &[
                (
                    "durations.py",
                    "def parse_duration(text):\n    raise NotImplementedError\n",
                ),
                (
                    "check.py",
                    "from durations import parse_duration\nassert parse_duration('1h30m') == 5400\nassert parse_duration('45s') == 45\nassert parse_duration('2h') == 7200\nassert parse_duration('90m') == 5400\nassert parse_duration('1h1m1s') == 3661\nfor bad in ['', 'abc', '5', '1x']:\n    try:\n        parse_duration(bad)\n    except ValueError:\n        continue\n    raise AssertionError(bad)\n",
                ),
            ],
            &[(
                "durations.py",
                "import re\n\n\ndef parse_duration(text):\n    m = re.fullmatch(r'(?:(\\d+)h)?(?:(\\d+)m)?(?:(\\d+)s)?', text)\n    if not text or m is None:\n        raise ValueError(text)\n    h, mi, s = (int(g or 0) for g in m.groups())\n    return h * 3600 + mi * 60 + s\n",
            )],
        ),
        task(
            "parse-size",
            "Implement parse_size(text) in sizes.py: '10B', '2KB', '1MB' and '3GB' give the number of bytes (1 KB is 1024 bytes); a missing unit or an unknown unit raises ValueError. check.py shows the cases.",
            &[
                (
                    "sizes.py",
                    "def parse_size(text):\n    raise NotImplementedError\n",
                ),
                (
                    "check.py",
                    "from sizes import parse_size\nassert parse_size('10B') == 10\nassert parse_size('2KB') == 2048\nassert parse_size('1MB') == 1048576\nassert parse_size('3GB') == 3 * 1024 ** 3\nfor bad in ['10', 'KB', '5TB', '']:\n    try:\n        parse_size(bad)\n    except ValueError:\n        continue\n    raise AssertionError(bad)\n",
                ),
            ],
            &[(
                "sizes.py",
                "import re\n\nUNITS = {'B': 1, 'KB': 1024, 'MB': 1024 ** 2, 'GB': 1024 ** 3}\n\n\ndef parse_size(text):\n    m = re.fullmatch(r'(\\d+)([A-Z]+)', text)\n    if m is None or m.group(2) not in UNITS:\n        raise ValueError(text)\n    return int(m.group(1)) * UNITS[m.group(2)]\n",
            )],
        ),
        task(
            "parse-ranges",
            "Implement expand_ranges(text) in ranges.py: '1-3,5,7-8' gives [1, 2, 3, 5, 7, 8] (ascending, no duplicates); a reversed range such as '5-3' or a non-number raises ValueError. check.py shows the cases.",
            &[
                (
                    "ranges.py",
                    "def expand_ranges(text):\n    raise NotImplementedError\n",
                ),
                (
                    "check.py",
                    "from ranges import expand_ranges\nassert expand_ranges('1-3,5,7-8') == [1, 2, 3, 5, 7, 8]\nassert expand_ranges('4') == [4]\nassert expand_ranges('1-2,2-3') == [1, 2, 3]\nfor bad in ['5-3', 'a', '1-', '']:\n    try:\n        expand_ranges(bad)\n    except ValueError:\n        continue\n    raise AssertionError(bad)\n",
                ),
            ],
            &[(
                "ranges.py",
                "def expand_ranges(text):\n    out = set()\n    for part in text.split(','):\n        lo, sep, hi = part.partition('-')\n        lo_n = int(lo)\n        hi_n = int(hi) if sep else lo_n\n        if hi_n < lo_n:\n            raise ValueError(part)\n        out.update(range(lo_n, hi_n + 1))\n    return sorted(out)\n",
            )],
        ),
    ]
}

/// F9: collections.
fn collections() -> Vec<TrialTask> {
    vec![
        task(
            "dedupe-keep-order",
            "dedupe(items) in dedupe.py should drop repeated items but keep the first occurrence and the original order. It loses the order. Fix it so check.py passes.",
            &[
                (
                    "dedupe.py",
                    "def dedupe(items):\n    return list(set(items))\n",
                ),
                (
                    "check.py",
                    "from dedupe import dedupe\nassert dedupe([3, 1, 3, 2, 1]) == [3, 1, 2]\nassert dedupe([]) == []\nassert dedupe(['b', 'a', 'b']) == ['b', 'a']\n",
                ),
            ],
            &[(
                "dedupe.py",
                "def dedupe(items):\n    seen = set()\n    out = []\n    for item in items:\n        if item not in seen:\n            seen.add(item)\n            out.append(item)\n    return out\n",
            )],
        ),
        task(
            "merge-counts",
            "merge_counts(a, b) in counts.py should add the counts of two dicts key by key and return a new dict, leaving both inputs untouched. It overwrites instead of adding and mutates a. Fix it so check.py passes.",
            &[
                (
                    "counts.py",
                    "def merge_counts(a, b):\n    a.update(b)\n    return a\n",
                ),
                (
                    "check.py",
                    "from counts import merge_counts\na = {'x': 1, 'y': 2}\nb = {'y': 3, 'z': 4}\nassert merge_counts(a, b) == {'x': 1, 'y': 5, 'z': 4}\nassert a == {'x': 1, 'y': 2}\nassert b == {'y': 3, 'z': 4}\n",
                ),
            ],
            &[(
                "counts.py",
                "def merge_counts(a, b):\n    out = dict(a)\n    for k, v in b.items():\n        out[k] = out.get(k, 0) + v\n    return out\n",
            )],
        ),
        task(
            "flatten-nested",
            "flatten(value) in flat.py should flatten arbitrarily nested lists into one list, in order, and leave strings whole. It only flattens one level. Fix it so check.py passes.",
            &[
                (
                    "flat.py",
                    "def flatten(value):\n    out = []\n    for item in value:\n        if isinstance(item, list):\n            out.extend(item)\n        else:\n            out.append(item)\n    return out\n",
                ),
                (
                    "check.py",
                    "from flat import flatten\nassert flatten([1, [2, [3, [4]]], 5]) == [1, 2, 3, 4, 5]\nassert flatten(['ab', ['cd']]) == ['ab', 'cd']\nassert flatten([]) == []\n",
                ),
            ],
            &[(
                "flat.py",
                "def flatten(value):\n    out = []\n    for item in value:\n        if isinstance(item, list):\n            out.extend(flatten(item))\n        else:\n            out.append(item)\n    return out\n",
            )],
        ),
    ]
}

/// F10: two modules that disagree.
fn interplay() -> Vec<TrialTask> {
    vec![
        task(
            "cart-discount-order",
            "The cart total in cart.py applies the discount from pricing.py before adding shipping, but the shipping should be added after the discount and must not itself be discounted; today it is. The numbers in check.py are right. Fix the code, not the check.",
            &[
                (
                    "pricing.py",
                    "def discount(amount, percent):\n    return round(amount * (1 - percent / 100), 2)\n",
                ),
                (
                    "cart.py",
                    "from pricing import discount\n\nSHIPPING = 5.0\n\n\ndef total(prices, percent):\n    return discount(sum(prices) + SHIPPING, percent)\n",
                ),
                (
                    "check.py",
                    "from cart import total\nassert total([10, 20], 10) == 32.0\nassert total([100], 0) == 105.0\n",
                ),
            ],
            &[(
                "cart.py",
                "from pricing import discount\n\nSHIPPING = 5.0\n\n\ndef total(prices, percent):\n    return round(discount(sum(prices), percent) + SHIPPING, 2)\n",
            )],
        ),
        task(
            "invoice-rounding",
            "line_total in invoice.py rounds each line before summing in invoice_total, which drifts by a cent; the total should be the rounded sum of the exact values. The numbers in check.py are right. Fix the code, not the check.",
            &[
                (
                    "invoice.py",
                    "def line_total(qty, price):\n    return round(qty * price, 2)\n\n\ndef invoice_total(lines):\n    return round(sum(line_total(q, p) for q, p in lines), 2)\n",
                ),
                (
                    "check.py",
                    "from invoice import invoice_total\nassert invoice_total([(1, 0.004), (1, 0.004), (1, 0.004)]) == 0.01\nassert invoice_total([(3, 0.333)]) == 1.0\n",
                ),
            ],
            &[(
                "invoice.py",
                "def line_total(qty, price):\n    return qty * price\n\n\ndef invoice_total(lines):\n    return round(sum(line_total(q, p) for q, p in lines), 2)\n",
            )],
        ),
        task(
            "inventory-reserve",
            "reserve(stock, sku, qty) in inventory.py should refuse to reserve more than is in stock with a ValueError and otherwise reduce the stock, but report.py's low_stock reads the wrong field so check.py fails. Fix whichever code is wrong so check.py passes.",
            &[
                (
                    "inventory.py",
                    "def reserve(stock, sku, qty):\n    if stock[sku]['on_hand'] < qty:\n        raise ValueError(sku)\n    stock[sku]['on_hand'] -= qty\n    return stock[sku]['on_hand']\n",
                ),
                (
                    "report.py",
                    "def low_stock(stock, threshold):\n    return sorted(sku for sku, row in stock.items() if row['reserved'] < threshold)\n",
                ),
                (
                    "check.py",
                    "from inventory import reserve\nfrom report import low_stock\nstock = {'a': {'on_hand': 10}, 'b': {'on_hand': 2}}\nassert reserve(stock, 'a', 4) == 6\ntry:\n    reserve(stock, 'b', 3)\nexcept ValueError:\n    pass\nelse:\n    raise AssertionError('over-reserve')\nassert low_stock(stock, 5) == ['b']\n",
                ),
            ],
            &[(
                "report.py",
                "def low_stock(stock, threshold):\n    return sorted(sku for sku, row in stock.items() if row['on_hand'] < threshold)\n",
            )],
        ),
    ]
}

/// The thirty tasks.
#[must_use]
pub fn builtin_suite() -> Vec<TrialTask> {
    let mut v = Vec::new();
    v.extend(off_by_one());
    v.extend(stubs());
    v.extend(renames());
    v.extend(validation());
    v.extend(mutable_defaults());
    v.extend(configuration());
    v.extend(ordering());
    v.extend(parsers());
    v.extend(collections());
    v.extend(interplay());
    v
}

/// Write a task's starting repository into `dir` and commit it.
///
/// # Errors
/// Any filesystem or git failure.
pub fn materialize(task: &TrialTask, dir: &std::path::Path) -> std::io::Result<()> {
    for (path, content) in &task.files {
        let full = dir.join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(full, content)?;
    }
    let git = |args: &[&str]| -> std::io::Result<()> {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other(format!("git {args:?} failed")))
        }
    };
    git(&["init", "-q", "-b", "main"])?;
    git(&["config", "core.autocrlf", "false"])?;
    git(&["add", "-A"])?;
    git(&[
        "-c",
        "user.name=trial",
        "-c",
        "user.email=trial@example.invalid",
        "commit",
        "-q",
        "-m",
        "start",
    ])
}

/// Apply a task's reference solution in `dir`.
///
/// # Errors
/// Any filesystem failure.
pub fn apply_reference(task: &TrialTask, dir: &std::path::Path) -> std::io::Result<()> {
    for (path, content) in &task.reference {
        std::fs::write(dir.join(path), content)?;
    }
    Ok(())
}

/// Run a task's check in `dir`; whether it passed.
///
/// # Errors
/// When `python3` cannot be run.
pub fn check_passes(dir: &std::path::Path) -> std::io::Result<bool> {
    Ok(std::process::Command::new(CHECK_ARGV[0])
        .arg(CHECK_ARGV[1])
        .current_dir(dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?
        .success())
}
