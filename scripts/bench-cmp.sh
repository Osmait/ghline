#!/bin/sh
# The same benchmarks either side of your uncommitted changes, on this machine,
# minutes apart.
#
# Doing it by hand is four commands, one of which puts your work in a stash,
# and the failure mode is quiet: the first attempt at the comparison that
# motivated this script produced a table where a function nobody had touched
# had got 50% slower, because something else on the desk woke up. A number
# read off that table is worse than no number.
#
# So the ratio for *every* benchmark is printed, not just the ones you meant to
# change. The ones you did not touch are the control, and the line at the end
# says what they did.
#
# Usage: scripts/bench-cmp.sh [divan filter ...]

set -eu

cd "$(git rev-parse --show-toplevel)"

if git diff --quiet -- src crates && git diff --cached --quiet -- src crates; then
	echo "  nothing is changed under src/ or crates/ — there is nothing to compare against" >&2
	exit 1
fi

before=target/bench-before.txt
after=target/bench-after.txt
mkdir -p target

# Only `src` and `crates` are stashed. The benchmarks themselves have to be the
# same on both sides or the comparison is between two different questions, and
# a bench file that names something old code does not have would fail to build.
echo "  stashing your changes under src/ and crates/ — if this is interrupted, they are in \`git stash list\`"
stash_before="$(git rev-parse -q --verify refs/stash || true)"
git stash push --quiet -- src crates
stash_after="$(git rev-parse -q --verify refs/stash || true)"
stashed=0
if [ "$stash_after" != "$stash_before" ]; then
	stashed=1
fi

# Restores the tree once on exit, including on a failed build or a ^C.
# Tracking `stashed` ensures we never pop an unrelated older stash or pop twice.
restore() {
	if [ "$stashed" -eq 1 ]; then
		stashed=0
		git stash pop --quiet
	fi
}
trap restore EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

echo "  measuring HEAD…"
cargo bench --quiet -- "$@" >"$before" 2>&1 || {
	echo "  the benchmarks did not build against HEAD" >&2
	sed -n '1,20p' "$before" >&2
	exit 1
}

restore
trap - EXIT INT TERM

echo "  measuring your working tree…"
cargo bench --quiet -- "$@" >"$after" 2>&1 || {
	echo "  the benchmarks did not build" >&2
	sed -n '1,20p' "$after" >&2
	exit 1
}

echo

# divan draws a tree with box-drawing characters, and the vertical bar it
# separates columns with is the same one the tree is made of — so the columns
# cannot be split on it. Stripping the box characters first leaves
# `name fastest unit slowest unit median unit mean unit samples iters`, which
# is positional and unambiguous.
awk '
function ns(v, u) {
	if (u == "ns") return v
	if (u == "µs" || u == "us") return v * 1000
	if (u == "ms") return v * 1000000
	if (u == "s")  return v * 1000000000
	return v
}
function pretty(v) {
	if (v >= 1000000) return sprintf("%.3g ms", v / 1000000)
	if (v >= 1000)    return sprintf("%.4g µs", v / 1000)
	return sprintf("%.4g ns", v)
}
{
	line = $0
	gsub(/[│├╰─┬╭╮╯]/, " ", line)
	n = split(line, f, /[ \t]+/)
	# split leaves an empty first field when the line began with whitespace
	i = (f[1] == "" ? 2 : 1)
	name = f[i]
	if (name == "") next
	# a group heading carries no numbers; the header row carries no numbers
	# either, and `fastest` is not one
	if (n - i < 4 || f[i + 1] !~ /^[0-9]+(\.[0-9]+)?$/) { group = name; next }
	key = group "::" name
	fast = ns(f[i + 1], f[i + 2])
	med  = ns(f[i + 5], f[i + 6])
	if (pass == 1) { bf[key] = fast; bm[key] = med; order[++k] = key; spread1[++s1] = med / fast }
	else           { af[key] = fast; am[key] = med; spread2[++s2] = med / fast }
}
function middle(a, n,   i, j, v) {
	for (i = 2; i <= n; i++) { v = a[i]; j = i - 1
		while (j > 0 && a[j] > v) { a[j + 1] = a[j]; j-- }
		a[j + 1] = v }
	return (n % 2) ? a[(n + 1) / 2] : (a[n / 2] + a[n / 2 + 1]) / 2
}
END {
	printf "%-32s %14s %14s %9s %9s\n", "", "median before", "median after", "×median", "×fastest"
	nr = 0
	for (j = 1; j <= k; j++) {
		key = order[j]
		if (!(key in am) || am[key] == 0 || af[key] == 0) continue
		rm = bm[key] / am[key]
		rf = bf[key] / af[key]
		printf "%-32s %14s %14s %9.2f %9.2f\n", key, pretty(bm[key]), pretty(am[key]), rm, rf
		nr++
	}
	if (nr == 0) { print "  no benchmark ran on both sides"; exit }

	# How far each run'"'"'s median sat above its own fastest sample. Interference
	# can only ever make a sample slower, so this is the run saying how much of
	# it was somebody else — and unlike comparing benchmarks against each other,
	# it does not assume that most of them were left alone. The first attempt at
	# this comparison read 13% and 25% here, on a desk that looked idle.
	a = (middle(spread1, s1) - 1) * 100
	b = (middle(spread2, s2) - 1) * 100

	printf "\n  × is how many times faster it now is; 1.00 is unchanged.\n"
	printf "  interference: %.0f%% measuring HEAD, %.0f%% measuring your tree.\n", a, b
	if (a > 5 || b > 5)
		print "  one of the runs was disturbed — a benchmark you did not touch\n  having moved is the tell. Read it again on an idle machine."
	else
		print "  both runs were steady, so a row that moved, moved because of the code."
}
' pass=1 "$before" pass=2 "$after"
