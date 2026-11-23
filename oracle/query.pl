#!/usr/bin/perl
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
