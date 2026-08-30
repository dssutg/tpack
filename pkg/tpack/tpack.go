package tpack

import (
	"encoding/json"
	"fmt"
	"image"
	"image/draw"
	"image/png"
	"math"
	"os"
	"path/filepath"
	"slices"
	"strings"
)

type MapFormat int

const (
	MapFormatAuto MapFormat = iota
	MapFormatC
	MapFormatStructuredC
	MapFormatJSON
	MapFormatSSM
)

var stringToMapFormat = map[string]MapFormat{
	"auto":         MapFormatAuto,
	"c":            MapFormatC,
	"structured-c": MapFormatStructuredC,
	"json":         MapFormatJSON,
	"ssm":          MapFormatSSM,
}

var extToMapFormat = map[string]MapFormat{
	".json": MapFormatJSON,
	".ssm":  MapFormatSSM,
	".c":    MapFormatC,
}

func ValidMapFormatStrings() []string {
	formats := make([]string, 0, len(stringToMapFormat))
	for format := range stringToMapFormat {
		formats = append(formats, format)
	}
	slices.Sort(formats)
	return formats
}

func MapFormatFromString(s string) (format MapFormat, ok bool) {
	format, ok = stringToMapFormat[s]
	return
}

const (
	MinAtlasWidth     = 8
	DefaultAtlasWidth = 2048
	MaxAtlasWidth     = 4096

	MinAtlasHeight     = 8
	DefaultAtlasHeight = 2048
	MaxAtlasHeight     = 4096

	MinSpritePadding     = 0
	DefaultSpritePadding = 0
	MaxSpritePadding     = 128

	DefaultMapFormat     = "auto"
	DefaultMapEnumPrefix = "SPRITE_"
	DefaultMapArrayName  = "sprites"
	DefaultMapArrayType  = "SDL_Rect"
)

type Options struct {
	Dir           string    // input directory path
	AtlasPath     string    // output texture atlas image path
	SpriteMapPath string    // output texture atlas sprite map path
	AtlasWidth    int       // width of texture atlas in pixels
	AtlasHeight   int       // height of texture atlas in pixels
	SpritePadding int       // pixel padding between sprites in output atlas
	MapFormat     MapFormat // atlas map format
	MapEnumPrefix string    // atlas map enum format (only for structured C format)
	MapArrayName  string    // atlas map array name (only for C formats)
	MapArrayType  string    // atlas map array type (only for Structured C format)
}

type Sprite struct {
	Offset    image.Point // sprite position inside atlas
	ImagePath string      // original sprite image file path
	Image     image.Image // sprite image (not in atlas)
	Packed    bool        // true if the sprite has been packed to atlas
}

type AtlasTooSmallError struct {
	Sprites []Sprite // all sprites (packed and not packed)
}

func (*AtlasTooSmallError) Error() string {
	return "failed to pack all sprites"
}

func PackSpritesToAtlas(opt Options) error {
	if opt.AtlasWidth < MinAtlasWidth || opt.AtlasWidth > MaxAtlasWidth {
		return fmt.Errorf("invalid atlas width (min: %d, max: %d)", MinAtlasWidth, MaxAtlasWidth)
	}
	if opt.AtlasHeight < MinAtlasHeight || opt.AtlasHeight > MaxAtlasHeight {
		return fmt.Errorf("invalid atlas height (min: %d, max: %d)", MinAtlasHeight, MaxAtlasHeight)
	}
	if opt.SpritePadding < MinSpritePadding || opt.SpritePadding > MaxSpritePadding {
		return fmt.Errorf("invalid sprite padding (min: %d, max: %d)", MinSpritePadding, MaxSpritePadding)
	}

	sprites, err := FindAndReadSprites(opt.Dir, ".png")
	if err != nil {
		return err
	}

	packed := PackSpritesSkyline(sprites, opt.AtlasWidth, opt.AtlasHeight, opt.SpritePadding)

	if packed != len(sprites) {
		return &AtlasTooSmallError{Sprites: sprites}
	}

	atlasImage := image.NewRGBA(image.Rect(0, 0, opt.AtlasWidth, opt.AtlasHeight))

	for i := range sprites {
		s := &sprites[i]
		b := s.Image.Bounds()
		destRect := image.Rectangle{Min: s.Offset, Max: s.Offset.Add(b.Size())}
		draw.Draw(atlasImage, destRect, s.Image, b.Min, draw.Over)
	}

	err = SaveTextureAtlas(opt.AtlasPath, atlasImage)
	if err != nil {
		return err
	}

	err = SaveAtlasSpriteMap(
		opt.MapFormat,
		opt.SpriteMapPath,
		opt.MapEnumPrefix,
		opt.MapArrayName,
		opt.MapArrayType,
		sprites,
	)
	if err != nil {
		return err
	}

	return nil
}

func FindAndReadSprites(path string, fileExt string) ([]Sprite, error) {
	filenames, err := FindImageFiles(path, fileExt)
	if err != nil {
		return nil, fmt.Errorf("cannot read sprite image files: %v", err)
	}

	var sprites []Sprite
	for _, path := range filenames {
		sprite, err := ReadSpriteFromFile(path)
		if err != nil {
			return nil, fmt.Errorf("cannot read sprite image file %s: %v", path, err)
		}

		sprites = append(sprites, sprite)
	}

	return sprites, nil
}

// FindImageFiles traverses the dirPath directory recursively
// and returns the slice of the all found files that has the
// extension. The extension parameter is assumed to start with dot.
// If there's an error, non-nil is returned.
func FindImageFiles(dirPath, extension string) ([]string, error) {
	var paths []string

	err := filepath.Walk(dirPath, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return err
		}

		if !info.IsDir() && strings.HasSuffix(strings.ToLower(info.Name()), extension) {
			paths = append(paths, path)
		}

		return nil
	})
	if err != nil {
		return nil, err
	}

	return paths, nil
}

func ReadSpriteFromFile(path string) (Sprite, error) {
	f, err := os.Open(path)
	if err != nil {
		return Sprite{}, err
	}
	defer f.Close()

	img, err := png.Decode(f)
	if err != nil {
		return Sprite{}, err
	}

	return Sprite{ImagePath: path, Image: img}, nil
}

func PackSpritesSkyline(sprites []Sprite, atlasWidth, atlasHeight, padding int) (packed int) {
	skyline := make([]int, atlasWidth)

	for i := range sprites {
		bestPoint := image.Pt(-1, math.MaxInt)
		size := sprites[i].Image.Bounds().Size().Add(image.Pt(padding, padding))

		for x := range atlasWidth - size.X + 1 {
			y := slices.Max(skyline[x : x+size.X])
			if y+size.Y <= atlasHeight && y < bestPoint.Y {
				bestPoint = image.Point{X: x, Y: y}
			}
		}

		if bestPoint.X != -1 {
			sprites[i].Offset = bestPoint
			sprites[i].Packed = true
			packed++
			for x := bestPoint.X; x < bestPoint.X+size.X; x++ {
				skyline[x] = bestPoint.Y + size.Y
			}
		}
	}

	return
}

func SaveTextureAtlas(atlasPath string, atlasImg *image.RGBA) error {
	err := os.MkdirAll(filepath.Dir(atlasPath), 0o755)
	if err != nil {
		return fmt.Errorf("error making directories for atlas to %s: %v", atlasPath, err)
	}

	f, err := os.Create(atlasPath)
	if err != nil {
		return fmt.Errorf("error creating texture atlas file: %v", err)
	}
	defer f.Close()

	err = png.Encode(f, atlasImg)
	if err != nil {
		return fmt.Errorf("error encoding output image: %v", err)
	}

	return nil
}

// imagePathToEnumName converts an image path (e.g., "/images/user-profile.png")
// into a standardized Enum name (e.g., "ImageUserProfile").
func imagePathToEnumName(name string) string {
	// 1. Remove the directory path
	base := filepath.Base(name)

	// 2. Remove the file extension (e.g., .png, .jpg)
	nameWithoutExt := strings.TrimSuffix(base, filepath.Ext(base))

	// 3. Replace invalid characters (like hyphens and spaces) with spaces
	replacer := strings.NewReplacer(
		"-", " ",
		"_", " ",
		".", " ",
	)
	cleanName := replacer.Replace(nameWithoutExt)

	// 4. Convert to PascalCase
	words := strings.Fields(cleanName)
	var enumBuilder strings.Builder

	for _, word := range words {
		if len(word) > 0 {
			enumBuilder.WriteString(strings.ToUpper(word[:1]))
			enumBuilder.WriteString(strings.ToLower(word[1:]))
		}
	}

	return enumBuilder.String()
}

func SaveAtlasSpriteMap(
	format MapFormat,
	spriteMapPath string,
	mapEnumPrefix string,
	mapArrayName string,
	mapArrayType string,
	sprites []Sprite,
) error {
	err := os.MkdirAll(filepath.Dir(spriteMapPath), 0o755)
	if err != nil {
		return fmt.Errorf("error making directories for sprite map to %s: %v", spriteMapPath, err)
	}

	f, err := os.Create(spriteMapPath)
	if err != nil {
		return fmt.Errorf("error creating sprite map file: %v", err)
	}
	defer f.Close()

	if format == MapFormatAuto {
		ext := strings.ToLower(filepath.Ext(spriteMapPath))
		format = extToMapFormat[ext]
	}

	switch format {
	case MapFormatJSON:
		// Even though it's a regular JSON, it's anyway better to
		// format it the way to make it more readable, compact, and easy
		// to parse it without even a full featured JSON parser. For example,
		// assuming you never have quotes, colons, and commas in sprite paths:
		// awk '!/^[{}]$/ {print $1" "$4" "$6" "$8" "$10}' sprites.json | tr -d '":,'
		f.WriteString("{\n")
		for i := range sprites {
			s := &sprites[i]
			name, err := json.Marshal(s.ImagePath)
			if err != nil {
				return fmt.Errorf("error marshalling sprite name %s in sprite map file: %v", s.ImagePath, err)
			}
			b := s.Image.Bounds()
			fmt.Fprintf(f, `  %s: { "x": %d, "y": %d, "w": %d, "h": %d }`,
				string(name), s.Offset.X, s.Offset.Y, b.Dx(), b.Dy(),
			)
			if i != len(sprites)-1 {
				f.WriteString(",")
			}
			f.WriteString("\n")
		}
		f.WriteString("}\n")
	case MapFormatSSM:
		// Simple Sprite Map format is intended for the languages
		// without built-in JSON support. So its structure is as
		// simple as possible to be parsed easily. The format is
		// tab separated values (TSV). If the filename has tabs or
		// backslash, they are escaped with a single backslash.
		// If you ensure that your image paths don't have tabs
		// and backslashes, you can easily parse this format
		// line by line using a split(' ')-like function.
		for i := range sprites {
			s := &sprites[i]
			b := s.Image.Bounds()
			name, err := json.Marshal(s.ImagePath)
			if err != nil {
				return fmt.Errorf("error marshalling sprite name %s in sprite map file: %v", s.ImagePath, err)
			}
			fmt.Fprintf(f, "S\t%s\t%d\t%d\t%d\t%d\n", string(name), s.Offset.X, s.Offset.Y, b.Dx(), b.Dy())
		}
	case MapFormatC:
		// Simple Sprite Map format is intended for the languages
		// without built-in JSON support. So its structure is as
		// simple as possible to be parsed easily. The format is
		// tab separated values (TSV). If the filename has tabs or
		// backslash, they are escaped with a single backslash.
		// If you ensure that your image paths don't have tabs
		// and backslashes, you can easily parse this format
		// line by line using a split(' ')-like function.
		f.WriteString("/* DO NOT EDIT; generated with tpack */\n")
		f.WriteString("\n")
		f.WriteString("const struct {\n")
		f.WriteString("  const char *filename;\n")
		f.WriteString("  int x, y, w, h;\n")
		fmt.Fprintf(f, "} %s[] = {\n", mapArrayName)
		for i := range sprites {
			s := &sprites[i]
			name, err := json.Marshal(s.ImagePath)
			if err != nil {
				return fmt.Errorf("error marshalling sprite name %s in sprite map file: %v", s.ImagePath, err)
			}
			b := s.Image.Bounds()
			fmt.Fprintf(f, "  { .filename = %s, .x = %d, .y = %d, .w = %d, .h = %d },\n",
				string(name), s.Offset.X, s.Offset.Y, b.Dx(), b.Dy(),
			)
		}
		f.WriteString("};\n")
	case MapFormatStructuredC:
		f.WriteString("/* DO NOT EDIT; generated with tpack */\n")
		f.WriteString("\n")
		f.WriteString("enum {\n")
		for i := range sprites {
			s := &sprites[i]
			name := imagePathToEnumName(s.ImagePath)
			fmt.Fprintf(f, "  %s%s,\n", mapEnumPrefix, string(name))
		}
		f.WriteString("};\n")
		f.WriteString("\n")
		fmt.Fprintf(f, "extern const %s %s[];\n", mapArrayType, mapArrayName)
		f.WriteString("\n")
		fmt.Fprintf(f, "#ifdef %s_IMPLEMENTATION\n", mapEnumPrefix)
		// Structured C format is intended for extremely fast sprite lookup.
		fmt.Fprintf(f, "const %s %s[] = {\n", mapArrayType, mapArrayName)
		for i := range sprites {
			s := &sprites[i]
			name := imagePathToEnumName(s.ImagePath)
			b := s.Image.Bounds()
			fmt.Fprintf(f, " [%s%s] = { .x = %d, .y = %d, .w = %d, .h = %d },\n",
				mapEnumPrefix, string(name), s.Offset.X, s.Offset.Y, b.Dx(), b.Dy(),
			)
		}
		f.WriteString("};\n")
		fmt.Fprintf(f, "#endif /* %s_IMPLEMENTATION */\n", mapEnumPrefix)
	default:
		return fmt.Errorf("unknown format of sprite map %s", spriteMapPath)
	}

	return nil
}
