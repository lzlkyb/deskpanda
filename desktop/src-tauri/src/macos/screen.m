#import <AppKit/AppKit.h>
#import <ScreenCaptureKit/ScreenCaptureKit.h>
#import <CoreGraphics/CoreGraphics.h>
#include <stdint.h>
#include <math.h>
@interface PPShotResult : NSObject
@property NSData *png;
@property int32_t status;
@end
@implementation PPShotResult
@end
static CGFloat primaryScale(void) {
    CGRect bounds = CGDisplayBounds(CGMainDisplayID());
    CGDisplayModeRef mode = CGDisplayCopyDisplayMode(CGMainDisplayID());
    CGFloat width = mode ? CGDisplayModeGetPixelWidth(mode) : CGDisplayPixelsWide(CGMainDisplayID());
    if (mode) CGDisplayModeRelease(mode);
    return bounds.size.width > 0 ? MAX(1, width/bounds.size.width) : 1;
}
int32_t pp_mac_monitors(uint8_t **output, size_t *length) {
    if (!output || !length) return 1;
    *output=NULL; *length=0;
    @autoreleasepool {
        CGDirectDisplayID displays[32]; uint32_t count=0;
        if (CGGetActiveDisplayList(32, displays, &count)!=kCGErrorSuccess || count==0) return 2;
        NSMutableArray *list=[NSMutableArray new]; CGFloat scale=primaryScale();
        for (uint32_t i=0; i<count; i++) {
            CGRect rect=CGDisplayBounds(displays[i]);
            if (CGDisplayMirrorsDisplay(displays[i])!=kCGNullDirectDisplay) continue;
            NSDictionary *item=@{@"index":@0, @"x":@(llround(rect.origin.x*scale)),
                @"y":@(llround(rect.origin.y*scale)), @"w":@(llround(rect.size.width*scale)),
                @"h":@(llround(rect.size.height*scale)), @"primary":(displays[i]==CGMainDisplayID()?@YES:@NO), @"displayId":@(displays[i]), @"scale":@(scale)};
            if (displays[i]==CGMainDisplayID()) [list insertObject:item atIndex:0]; else [list addObject:item];
        }
        for (NSUInteger i=0;i<list.count;i++) {
            NSMutableDictionary *item=[list[i] mutableCopy]; item[@"index"]=@(i); list[i]=item;
        }
        NSData *json=[NSJSONSerialization dataWithJSONObject:list options:0 error:nil];
        if (!json) return 3;
        uint8_t *bytes=malloc(json.length); if(!bytes) return 4;
        memcpy(bytes,json.bytes,json.length); *output=bytes; *length=json.length; return 0;
    }
}
void pp_mac_cursor(int32_t *x, int32_t *y) {
    CGEventRef event=CGEventCreate(NULL);
    CGPoint point=event ? CGEventGetLocation(event) : CGPointZero;
    if(event) CFRelease(event);
    CGFloat scale=primaryScale(); if(x) *x=(int32_t)llround(point.x*scale); if(y) *y=(int32_t)llround(point.y*scale);
}
extern int32_t pp_mac_capture_legacy(uint32_t displayID, uint32_t width, uint32_t height, uint8_t **output,size_t *length);
// Must be called from a worker, never the AppKit main thread. No permission prompts.
int32_t pp_mac_capture_display(uint32_t displayID, uint32_t width, uint32_t height, uint8_t **output, size_t *length) {
    if(!output || !length) return 1;
    *output=NULL; *length=0;
    if(!width || !height || width>67108864/height) return 6;
    if(NSThread.isMainThread) return 7;
    if(!CGPreflightScreenCaptureAccess()) return 2;
    if (@available(macOS 14.0, *)) {
        @autoreleasepool {
            PPShotResult *result=[PPShotResult new]; result.status=3;
            dispatch_semaphore_t done=dispatch_semaphore_create(0);
            [SCShareableContent getShareableContentWithCompletionHandler:^(SCShareableContent *content, NSError *error) {
                if(error || !content) { dispatch_semaphore_signal(done); return; }
                SCDisplay *display=nil;
                for(SCDisplay *candidate in content.displays) if(candidate.displayID==displayID) display=candidate;
                if(!display) { dispatch_semaphore_signal(done); return; }
                NSMutableArray *excluded=[NSMutableArray new];
                for(SCRunningApplication *app in content.applications)
                    if(app.processID==getpid()) [excluded addObject:app];
                SCContentFilter *filter=[[SCContentFilter alloc] initWithDisplay:display excludingApplications:excluded exceptingWindows:@[]];
                SCStreamConfiguration *config=[SCStreamConfiguration new];
                config.width=width;
                config.height=height;
                if(config.width==0 || config.height==0 || config.width > 67108864/config.height) {
                    result.status=6; dispatch_semaphore_signal(done); return;
                }
                config.colorSpaceName=kCGColorSpaceSRGB;
                config.showsCursor=NO;
                [SCScreenshotManager captureImageWithFilter:filter configuration:config completionHandler:^(CGImageRef image, NSError *captureError) {
                    if(image && !captureError) {
                        NSBitmapImageRep *bitmap=[[NSBitmapImageRep alloc] initWithCGImage:image];
                        result.png=[bitmap representationUsingType:NSBitmapImageFileTypePNG properties:@{}];
                        if(result.png) result.status=0;
                    }
                    dispatch_semaphore_signal(done);
                }];
            }];
            if(dispatch_semaphore_wait(done,dispatch_time(DISPATCH_TIME_NOW,8*NSEC_PER_SEC))!=0) return 4;
            if(result.status!=0) return result.status;
            if(result.png.length > 256*1024*1024) return 6;
            uint8_t *bytes=malloc(result.png.length); if(!bytes) return 5;
            memcpy(bytes,result.png.bytes,result.png.length); *output=bytes; *length=result.png.length; return 0;
        }
    }
    return pp_mac_capture_legacy(displayID,width,height,output,length);
}

// Shared visibility/owner rules for snapping and scroll targeting.
static NSArray *externalWindowInfo(void) {
    NSArray *windows=CFBridgingRelease(CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements, kCGNullWindowID));
    if(!windows) return nil;
    NSMutableArray *visible=[NSMutableArray new];
    for(NSDictionary *info in windows) {
        int32_t owner=[info[(__bridge id)kCGWindowOwnerPID] intValue];
        CGRect bounds;
        if(owner<=0 || owner==getpid() || [info[(__bridge id)kCGWindowLayer] intValue]!=0 ||
           [info[(__bridge id)kCGWindowAlpha] doubleValue]<=0) continue;
        if(!CGRectMakeWithDictionaryRepresentation((__bridge CFDictionaryRef)info[(__bridge id)kCGWindowBounds],&bounds) ||
           bounds.size.width<16 || bounds.size.height<16) continue;
        [visible addObject:info];
    }
    return visible;
}
int32_t pp_mac_window_rects(uint8_t **output, size_t *length) {
    if(!output || !length) return 1;
    *output=NULL; *length=0;
    if(!CGPreflightScreenCaptureAccess()) return 2;
    @autoreleasepool {
        NSArray *windows=externalWindowInfo();
        if(!windows) return 3;
        CGFloat scale=primaryScale(); NSMutableArray *rects=[NSMutableArray new];
        for(NSDictionary *window in windows) {
            CGRect bounds;
            if(!CGRectMakeWithDictionaryRepresentation((__bridge CFDictionaryRef)window[(__bridge id)kCGWindowBounds], &bounds) ||
               bounds.size.width<16 || bounds.size.height<16) continue;
            [rects addObject:@{@"x":@(llround(bounds.origin.x*scale)), @"y":@(llround(bounds.origin.y*scale)),
                              @"w":@(llround(bounds.size.width*scale)), @"h":@(llround(bounds.size.height*scale))}];
        }
        NSData *json=[NSJSONSerialization dataWithJSONObject:rects options:0 error:nil];
        if(!json || json.length>4*1024*1024) return 3;
        uint8_t *bytes=malloc(json.length); if(!bytes) return 5;
        memcpy(bytes,json.bytes,json.length); *output=bytes; *length=json.length; return 0;
    }
}

// Resolve the topmost external window. A batch retains both its PID and window ID.
int32_t pp_mac_scroll_target_at(int32_t x, int32_t y, int32_t *pid, uint32_t *windowID) {
    if(!pid || !windowID) return 3;
    *pid=0; *windowID=0;
    @autoreleasepool {
        CGPoint point=CGPointMake(x/primaryScale(), y/primaryScale());
        NSArray *windows=externalWindowInfo();
        for(NSDictionary *info in windows) {
            int32_t owner=[info[(__bridge id)kCGWindowOwnerPID] intValue];
            CGRect rect;
            if(CGRectMakeWithDictionaryRepresentation((__bridge CFDictionaryRef)info[(__bridge id)kCGWindowBounds], &rect) && CGRectContainsPoint(rect,point)) {
                *pid=owner; *windowID=[info[(__bridge id)kCGWindowNumber] unsignedIntValue];
                return *windowID ? 0 : 2;
            }
        }
        return 2;
    }
}
extern bool pp_mac_target_valid(int32_t pid, uint32_t ownPID);
int32_t pp_mac_scroll_send(int32_t pid, uint32_t windowID, int32_t x, int32_t y, int32_t lines) {
    if(!AXIsProcessTrusted()) return 1;
    if(pid<=0 || pid==getpid() || windowID==0 || lines==0 || lines>120 || lines < -120) return 2;
    if(!pp_mac_target_valid(pid,getpid()) || NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier!=pid) return 2;
    int32_t currentPID=0; uint32_t currentWindow=0;
    if(pp_mac_scroll_target_at(x,y,&currentPID,&currentWindow)!=0 || currentPID!=pid || currentWindow!=windowID) return 2;
    CGEventRef event=CGEventCreateScrollWheelEvent(NULL,kCGScrollEventUnitLine,1,lines);
    if(!event) return 3;
    CGEventSetLocation(event,CGPointMake(x/primaryScale(),y/primaryScale()));
    CGEventSetFlags(event,0);
    CGEventPostToPid(pid,event);
    CFRelease(event);
    return 0;
}
double pp_mac_screen_scale(void) { return primaryScale(); }

uint32_t pp_mac_display_hz(uint32_t display){
    CGDisplayModeRef mode=CGDisplayCopyDisplayMode(display);double hz=mode?CGDisplayModeGetRefreshRate(mode):0;if(mode)CGDisplayModeRelease(mode);
    if(hz>0 && hz<=1000)return (uint32_t)llround(hz);
    if(!NSApp)return 0;__block uint32_t maximum=0;
    void (^read)(void)=^{for(NSScreen *screen in NSScreen.screens)if([screen.deviceDescription[@"NSScreenNumber"] unsignedIntValue]==display){if(@available(macOS 12.0,*))maximum=(uint32_t)screen.maximumFramesPerSecond;}};
    if(NSThread.isMainThread)read();else dispatch_sync(dispatch_get_main_queue(),read);return maximum;
}

// 共享的原生内存释放（screen/remote_capture/remote_video/wic_jpeg 等的
// read_native 归一出口；原仓住在 paste.m，这里随符号家族归入 screen.m）。
void pp_mac_free(void *memory) { free(memory); }

// ===== ② 远端分辨率调节（2026-11-22）：显示模式枚举 / 切换 / 会话期还原 =====
// 枚举只给 CGDisplayModeIsUsableForDesktopGUI 的模式（滤掉 HiDPI 双倍重复档）；
// w/h 用**像素**宽高（与 SCK 采集同口径）。current 标记当前模式。

int32_t pp_mac_display_modes(uint32_t displayID, uint8_t **output, size_t *length) {
    if(!output || !length) return 1;
    *output=NULL; *length=0;
    @autoreleasepool {
        CFArrayRef all=CGDisplayCopyAllDisplayModes(displayID,NULL);
        if(!all) return 2;
        CGDisplayModeRef cur=CGDisplayCopyDisplayMode(displayID);
        NSMutableArray *list=[NSMutableArray new];
        CFIndex n=CFArrayGetCount(all);
        for(CFIndex i=0;i<n;i++){
            CGDisplayModeRef mode=(CGDisplayModeRef)CFArrayGetValueAtIndex(all,i);
            if(!CGDisplayModeIsUsableForDesktopGUI(mode)) continue;
            double rate=CGDisplayModeGetRefreshRate(mode);
            uint32_t hz=(rate>0&&rate<=1000)?(uint32_t)llround(rate):0;
            // 🔴 不能用指针相等比「当前模式」:CGDisplayCopyDisplayMode 返回的
            // 是新 ref,与数组里的同模式实例指针必不相等——按像素尺寸+刷新率比。
            BOOL current=NO;
            if(cur){
                double curRate=CGDisplayModeGetRefreshRate(cur);
                uint32_t curHz=(curRate>0&&curRate<=1000)?(uint32_t)llround(curRate):0;
                current=(CGDisplayModeGetPixelWidth(mode)==CGDisplayModeGetPixelWidth(cur)
                      && CGDisplayModeGetPixelHeight(mode)==CGDisplayModeGetPixelHeight(cur)
                      && hz==curHz);
            }
            [list addObject:@{
                @"w":@(CGDisplayModeGetPixelWidth(mode)),
                @"h":@(CGDisplayModeGetPixelHeight(mode)),
                @"hz":@(hz),
                @"current":@(current),
            }];
        }
        if(cur) CGDisplayModeRelease(cur);
        CFRelease(all);
        NSData *json=[NSJSONSerialization dataWithJSONObject:list options:0 error:nil];
        if(!json) return 3;
        uint8_t *bytes=malloc(json.length); if(!bytes) return 4;
        memcpy(bytes,json.bytes,json.length); *output=bytes; *length=json.length; return 0;
    }
}

// 切换到第一个匹配 w/h（hz>0 时还要求刷新率一致）的可用模式。
// kCGConfigureForAppOnly：应用退出（含崩溃）时 OS 自动还原——这是 mac 侧
// 崩溃兜底，Drop 主动还原仍照做（会话正常结束就该回去，不等进程退出）。
// 成功输出空串，失败输出一行 JSON 错误文案（UTF-8）。
int32_t pp_mac_set_display_mode(uint32_t displayID,int32_t w,int32_t h,uint32_t hz,
                               uint8_t **output,size_t *length){
    if(!output || !length) return 1;
    *output=NULL; *length=0;
    if(w<=0||h<=0) { const char *m=("显示模式尺寸无效");
        *length=strlen(m); *output=(uint8_t*)strdup(m); return 2; }
    @autoreleasepool {
        CFArrayRef all=CGDisplayCopyAllDisplayModes(displayID,NULL);
        if(!all){ const char *m=("读不到显示器可用模式");
            *length=strlen(m); *output=(uint8_t*)strdup(m); return 3; }
        CGDisplayModeRef best=NULL;
        CFIndex n=CFArrayGetCount(all);
        for(CFIndex i=0;i<n;i++){
            CGDisplayModeRef mode=(CGDisplayModeRef)CFArrayGetValueAtIndex(all,i);
            if(!CGDisplayModeIsUsableForDesktopGUI(mode)) continue;
            if((int32_t)CGDisplayModeGetPixelWidth(mode)!=w) continue;
            if((int32_t)CGDisplayModeGetPixelHeight(mode)!=h) continue;
            double rate=CGDisplayModeGetRefreshRate(mode);
            uint32_t mhz=(rate>0&&rate<=1000)?(uint32_t)llround(rate):0;
            if(hz>0 && mhz!=hz) continue;
            best=mode;
            if(hz==0 || mhz==hz) break; // hz==0：第一个尺寸匹配即可
        }
        if(!best){ CFRelease(all);
            const char *m=("显示器不支持该分辨率（或该尺寸无可用模式）");
            *length=strlen(m); *output=(uint8_t*)strdup(m); return 5; }
        CGDisplayConfigRef config=NULL;
        CGError e=CGBeginDisplayConfiguration(&config);
        if(e!=kCGErrorSuccess){ CFRelease(all);
            const char *m=("开始显示配置失败");
            *length=strlen(m); *output=(uint8_t*)strdup(m); return 6; }
        e=CGConfigureDisplayWithDisplayMode(config,displayID,best,NULL);
        if(e!=kCGErrorSuccess){ CGCancelDisplayConfiguration(config); CFRelease(all);
            const char *m=("应用显示模式失败（可能被系统设置/托管配置拦下）");
            *length=strlen(m); *output=(uint8_t*)strdup(m); return 7; }
        e=CGCompleteDisplayConfiguration(config,kCGConfigureForAppOnly);
        CFRelease(all);
        if(e!=kCGErrorSuccess){
            const char *m=("确认显示配置失败");
            *length=strlen(m); *output=(uint8_t*)strdup(m); return 8;
        }
        const char *m="";
        *length=0; *output=(uint8_t*)strdup(m); return 0;
    }
}
