#!/usr/bin/perl
# Runs `systemd-analyze calendar` for each case on stdin and prints one TSV row per case.
#
# Input row:  <timezone> TAB <base time> TAB <expression>
# Output row: <timezone> TAB <base time> TAB <expression> TAB <status> TAB <normalized> TAB <elapses>
#
#   status      "ok"          systemd parsed the expression and computed its elapses
#               "error"       systemd rejected the expression
#               "unresolved"  systemd parsed the expression but gave up computing an elapse
#   normalized  the normalized form, or "-" when systemd did not print one
#   elapses     comma-separated UTC instants (RFC 3339, second precision),
#               or "never" when the expression never elapses after the base time,
#               or "-" when status is not "ok"
#
# Lines that are empty or start with "#" pass through unchanged.
use strict;
use warnings;

my $iterations = shift @ARGV // 5;
$iterations =~ /^[1-9][0-9]*$/ or die "usage: oracle-query [ITERATIONS] < cases.tsv\n";

my ($version) = `systemd-analyze --version` =~ /^(systemd \S+ \(\S+\))/
	or die "cannot read the systemd version\n";
print "# oracle: $version, iterations=$iterations\n";

while (my $line = <STDIN>) {
	my $n = $.;
	chomp $line;
	if ($line eq '' || $line =~ /^#/) {
		print "$line\n";
		next;
	}
	my ($tz, $base, $expr) = split /\t/, $line, -1;
	defined $expr or die "line $n: expected 3 tab-separated fields\n";

	my $pid = open(my $out, '-|') // die "fork: $!\n";
	if ($pid == 0) {
		open(STDERR, '>&', \*STDOUT) or die "dup: $!\n";
		$ENV{TZ} = $tz;
		exec('systemd-analyze', 'calendar', "--iterations=$iterations", "--base-time=$base", '--', $expr)
			or die "exec: $!\n";
	}
	my @lines = <$out>;
	close $out;
	my $exit = $? >> 8;

	my ($normalized, @local, @utc);
	for (@lines) {
		if (/^\s*Normalized form: (.*)$/) { $normalized = $1 }
		elsif (/^\s*(?:Next elapse|Iteration #\d+): (.*)$/) { push @local, $1 }
		elsif (/^\s*\(in UTC\): (.*)$/) { push @utc, $1 }
	}

	if ($exit != 0) {
		# The two failures mean different things, so an unrecognized one must not pass as either.
		my $status =
			  (grep { /^Failed to parse calendar specification / } @lines) ? 'error'
			: (grep { /^Failed to determine next elapse / } @lines)        ? 'unresolved'
			: die "line $n: unrecognized failure for '$expr': @lines";
		print join("\t", $tz, $base, $expr, $status, $normalized // '-', '-'), "\n";
		next;
	}
	defined $normalized or die "line $n: no normalized form in the output for '$expr'\n";

	my $elapses;
	if (@local == 1 && $local[0] eq 'never') {
		$elapses = 'never';
	} else {
		# systemd prints the "(in UTC)" line only when the local zone is not UTC.
		my @instants = @utc ? @utc : @local;
		@instants == @local or die "line $n: mismatched UTC lines for '$expr'\n";
		$elapses = join(',', map {
			/^\w{3} (\d{4}-\d{2}-\d{2}) (\d{2}:\d{2}:\d{2}) UTC$/
				or die "line $n: unexpected timestamp '$_' for '$expr'\n";
			"$1T$2Z"
		} @instants);
	}
	print join("\t", $tz, $base, $expr, 'ok', $normalized, $elapses), "\n";
}
