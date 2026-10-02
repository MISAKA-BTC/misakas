// A small Objective-C bridge to Metal for misaka-palw-tir-exec's fused unit-row kernels. Compiles the kernel from source at first use
// (no offline toolchain needed), runs one dispatch per call on shared-storage buffers.
#import <Foundation/Foundation.h>
#import <Metal/Metal.h>
#include <stdint.h>
#include <string.h>

static id<MTLDevice> g_device;
static id<MTLCommandQueue> g_queue;
static id<MTLComputePipelineState> g_pipe;
static NSLock* g_lock;
static int g_state; // 0 = untried, 1 = ready, -1 = unavailable

static const char* KERNEL_SRC =
#include "unit_rows.inc"
;

static int ensure(void) {
    if (g_state != 0) return g_state;
    @autoreleasepool {
        g_lock = [[NSLock alloc] init];
        g_device = MTLCreateSystemDefaultDevice();
        if (!g_device) { g_state = -1; return g_state; }
        g_queue = [g_device newCommandQueue];
        NSError* err = nil;
        id<MTLLibrary> lib = [g_device newLibraryWithSource:[NSString stringWithUTF8String:KERNEL_SRC] options:nil error:&err];
        if (!lib) { g_state = -1; return g_state; }
        id<MTLFunction> fn = [lib newFunctionWithName:@"unit_rows"];
        if (!fn) { g_state = -1; return g_state; }
        g_pipe = [g_device newComputePipelineStateWithFunction:fn error:&err];
        g_state = g_pipe ? 1 : -1;
    }
    return g_state;
}

int tir_metal_available(void) {
    static NSObject* once;
    @synchronized([NSObject class]) { (void)once; return ensure() == 1; }
}

// 0 = done; 1 = a row left the kernel's range (re-run on the CPU); negative = unavailable or failed.
int tir_metal_unit_rows(int kind, const int32_t* x, uint32_t rows, uint32_t n, int64_t eps, int32_t* out) {
    @synchronized([NSObject class]) {
        if (ensure() != 1) return -1;
        @autoreleasepool {
            size_t count = (size_t)rows * (size_t)n;
            if (count == 0) return 0;
            int64_t params[3] = {kind, (int64_t)n, eps};
            int32_t flag = 0;
            id<MTLBuffer> bx = [g_device newBufferWithBytes:x length:count * 4 options:MTLResourceStorageModeShared];
            id<MTLBuffer> bo = [g_device newBufferWithLength:count * 4 options:MTLResourceStorageModeShared];
            id<MTLBuffer> bp = [g_device newBufferWithBytes:params length:sizeof(params) options:MTLResourceStorageModeShared];
            id<MTLBuffer> bf = [g_device newBufferWithBytes:&flag length:4 options:MTLResourceStorageModeShared];
            if (!bx || !bo || !bp || !bf) return -2;
            id<MTLCommandBuffer> cb = [g_queue commandBuffer];
            id<MTLComputeCommandEncoder> enc = [cb computeCommandEncoder];
            [enc setComputePipelineState:g_pipe];
            [enc setBuffer:bx offset:0 atIndex:0];
            [enc setBuffer:bo offset:0 atIndex:1];
            [enc setBuffer:bp offset:0 atIndex:2];
            [enc setBuffer:bf offset:0 atIndex:3];
            NSUInteger tg = 128; // a power of two, at most the 256 the kernel's scratch holds
            if (tg > g_pipe.maxTotalThreadsPerThreadgroup) tg = g_pipe.maxTotalThreadsPerThreadgroup;
            [enc dispatchThreadgroups:MTLSizeMake(rows, 1, 1) threadsPerThreadgroup:MTLSizeMake(tg, 1, 1)];
            [enc endEncoding];
            [cb commit];
            [cb waitUntilCompleted];
            if (cb.status != MTLCommandBufferStatusCompleted) return -3;
            if (*(int32_t*)bf.contents != 0) return 1;
            memcpy(out, bo.contents, count * 4);
            return 0;
        }
    }
}
