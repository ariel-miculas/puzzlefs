#!/usr/bin/perl
use strict;
use warnings;
use Data::Dumper;
use CBOR::XS;
use JSON;
use Getopt::Long;
use List::Util qw(sum);

my @tags = qw(10.25 10.26 10.27 10.28 10.29 10.30);
my $total_size;

use constant MB => (1024 * 1024);
use constant GB => (1024 * MB);

my $PUZZLEFS_BINARY = "\$HOME/work/cisco/puzzlefs/target/debug/puzzlefs";

my ($REPO, $BASEDIR);

sub read_file {
	my $filename = shift;
	open(my $fh, '<', $filename)
		or die "Could not open file '$filename' $!";

	my $data =join "", <$fh>;
	$data
}

sub get_layer_blob {
	my $tag = shift;
	my $image_ref_name = shift;
	my $json_file = "$tag/oci2/index.json";
	my $json_data = read_file($json_file);
	my $json = decode_json($json_data);
	my $layer_file;

	for my $manifest ($json->{'manifests'}->@*) {
		if ($manifest->{'annotations'}->{'org.opencontainers.image.ref.name'} eq $image_ref_name) {
			$manifest->{'digest'} =~ /sha256:(.*)/;
			$layer_file = $1;
			last;
		}
	}
	$layer_file;
}

sub get_metadata_blobs {
	my $tag = shift;
	my $image_ref_name = shift;

	my $layer_file = get_layer_blob($tag, "squashfs");
	my $cbor_file = "$tag/oci2/blobs/sha256/$layer_file";
	my $cbor_data = read_file($cbor_file);
	my @md_blobs;

	my $cbor = decode_cbor($cbor_data);
	for my $layer ($cbor->{'metadatas'}->@*) {
		# 8 bytes offset, each represented by 2 hexidecimal characters
		# +1 byte representing BlobRefKind
		push @md_blobs, substr(unpack ('H*', $layer), 2 * 8 + 2);
	}
	@md_blobs;
}

sub download {
	for my $tag (@tags) {
		qx{mkdir -p $tag/oci};
		qx{skopeo copy docker://$REPO/$BASEDIR/barehost:$tag oci:$tag/oci:barehost};
	}
}

sub unpack_oci {
	for my $tag (@tags) {
		qx{umoci unpack --rootless --keep-dirlinks --image $tag/oci:barehost $tag/rfs};
	}
}

sub unpack_oci2 {
	my ($min, $avg, $max) = @_;
	my $options = "";
	if (defined $min) {
		$options .= " --min=$min";
	}
	if (defined $avg) {
		$options .= " --avg=$avg";
	}
	if (defined $max) {
		$options .= " --max=$max";
	}
	for my $tag (@tags) {
		qx{$PUZZLEFS_BINARY build $options $tag/rfs/rootfs $tag/oci2 squashfs};
	}
}

sub clean_all {
	for my $tag (@tags) {
		qx{rm -r $tag};
	}
}

sub clean_oci2 {
	for my $tag (@tags) {
		qx{rm -r $tag/oci2};
	}
}

sub generate_stats {
	my $layers_oci;
	my $layers_oci2;
	for my $d (@tags) {
		my $dir = $d."/oci/blobs/sha256";
		opendir(my $dh, $dir) || die "Can't open $dir $!";
		while (readdir $dh) {
			next if ($_ eq "." || $_ eq "..");
			my $file = "$dir/$_";
			$layers_oci->{"oci"}->{$_}->{'nr_times'}++;
			my $size = (stat($file))[7]; # 7 is file size
			$layers_oci->{"oci"}->{$_}->{'size'} = $size;
			$total_size->{"oci"}->{$d} += $size;
		}

		$dir = $d."/oci2/blobs/sha256";
		opendir($dh, $dir) || die "Can't open $dir $!";
		while (readdir $dh) {
			next if ($_ eq "." || $_ eq "..");
			my $file = "$dir/$_";
			$layers_oci->{"oci2"}->{$_}->{'nr_times'}++;
			my $size = (stat($file))[7]; # 7 is file size
			$layers_oci->{"oci2"}->{$_}->{'size'} = $size;
			$total_size->{"oci2"}->{$d} += $size;
		}

	}
	$layers_oci;
}
sub calculate_saved_storage {
	my $saved;
	my $layers = shift;
	my $oci_ver = shift;
	my $h = $layers->{$oci_ver};
	for my $layer (keys $h->%*) {
		my $nr_times = $h->{$layer}->{'nr_times'};
		if ($nr_times > 1) {
			$saved += ($nr_times - 1) * $h->{$layer}->{'size'};
		}
	}
	return $saved;
}

sub print_stats {
	my $layers_oci = generate_stats();
	my $total_oci;

	for my $ver (qw(oci oci2)) {
		my $saved_oci->{$ver} = calculate_saved_storage($layers_oci, $ver);
		for my $size (keys $total_size->{$ver}->%*) {
			$total_oci->{$ver} += $total_size->{$ver}->{$size};
		}
		print "$ver, ".scalar @tags." tags\n";
		print "total size: ".$total_oci->{$ver} / MB."MB\n";
		print "average layer size: ".$total_oci->{$ver} / (scalar @tags) / MB."MB\n";
		print "mashed together: ".($total_oci->{$ver} - $saved_oci->{$ver})/MB."MB\n";
		print "saved: ".$saved_oci->{$ver}/MB."MB\n\n";
	}


	print "metadata size:\n";
	for my $tag (@tags) {
		my @blobs = get_metadata_blobs($tag, "squashfs");
		my @sizes = (map {(stat("$tag/oci2/blobs/sha256/$_"))[7]} @blobs);
		my $total_size = sum @sizes;
		print "$tag: ".$total_size / MB."MB\n";
	}
}

sub usage {
	"usage: $0 REPO BASEDIR"
}

my $stats;
my $rebuild;
my $puzzle;
my $cdc_min;
my $cdc_avg;
my $cdc_max;

GetOptions (
	"stats"  => \$stats, # print stats
	"rebuild"  => \$rebuild, # rebuild everything
	"puzzle"  => \$puzzle, # rebuild only puzzlefs
	"min=i" => \$cdc_min, # fastcdc min param
	"avg=i" => \$cdc_avg, # fastcdc avg param
	"max=i" => \$cdc_max, # fastcdc max param
)
or die("Error in command line arguments\n");

if (defined $rebuild) {
	die usage() if scalar @ARGV < 2;
	($REPO, $BASEDIR) = @ARGV;
	clean_all();
	download();
	unpack_oci();
	unpack_oci2($cdc_min, $cdc_avg, $cdc_max);
}

if (defined $puzzle) {
	clean_oci2();
	unpack_oci2($cdc_min, $cdc_avg, $cdc_max);
}

if (defined $stats) {
	print_stats();
}

