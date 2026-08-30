package main

import (
	"flag"
	"fmt"
	"os"
	"strings"

	"tpack/pkg/tpack"
)

func usage() {
	fmt.Fprintf(os.Stderr, "Usage: %s [options] input_directory output_atlas_image_filename output_atlas_sprite_map_filename\n", os.Args[0])
	flag.PrintDefaults()
}

func main() {
	var (
		width         = flag.Int("width", tpack.DefaultAtlasWidth, "Width of the texture atlas")
		height        = flag.Int("height", tpack.DefaultAtlasHeight, "Height of the texture atlas")
		spritePadding = flag.Int("sprite-padding", tpack.DefaultSpritePadding, "Padding of each sprite")
		mapFormatStr  = flag.String("map-format", tpack.DefaultMapFormat, "Sprite map format")
		mapEnumPrefix = flag.String("map-enum-prefix", tpack.DefaultMapEnumPrefix, "Enum prefix of sprite map (only for structured-c format)")
		mapArrayName  = flag.String("map-array-name", tpack.DefaultMapArrayName, "Array name of sprite map (only for C formats)")
		mapArrayType  = flag.String("map-array-type", tpack.DefaultMapArrayType, "Array type of sprite map (only for structured-c format)")
	)

	flag.Usage = usage

	flag.Parse()

	args := flag.Args()
	if len(args) < 3 {
		fmt.Fprintf(os.Stderr, "%s: missing required positional arguments\n", os.Args[0])
		usage()
		os.Exit(2)
	}

	mapFormat, ok := tpack.MapFormatFromString(*mapFormatStr)
	if !ok {
		fmt.Fprintf(os.Stderr, "%s: invalid map format. Valid formats: %s\n",
			os.Args[0],
			strings.Join(tpack.ValidMapFormatStrings(), ", "))
		os.Exit(2)
	}

	err := tpack.PackSpritesToAtlas(tpack.Options{
		Dir:           args[0],
		AtlasPath:     args[1],
		SpriteMapPath: args[2],
		AtlasWidth:    *width,
		AtlasHeight:   *height,
		SpritePadding: *spritePadding,
		MapFormat:     mapFormat,
		MapEnumPrefix: *mapEnumPrefix,
		MapArrayName:  *mapArrayName,
		MapArrayType:  *mapArrayType,
	})
	if err != nil {
		fmt.Fprintf(os.Stderr, "%s: %v\n", os.Args[0], err)

		if err, ok := err.(*tpack.AtlasTooSmallError); ok {
			for _, sprite := range err.Sprites {
				if !sprite.Packed {
					size := sprite.Image.Bounds().Size()
					fmt.Fprintf(os.Stderr, "  FAILED %s [%dx%d]\n", sprite.ImagePath, size.X, size.Y)
				}
			}
		}

		os.Exit(1)
	}
}
