// DeskPanda local encryption key（S6 迁入，2026-11-21）。
// 提取自原仓 pastePanda 的 paste.m——那边它混在 paste 引擎本体里；本仓只需要
// keychain 的读/建两个入口（`macos/secret_store.rs` 经 extern 调用）。
// 服务名与标签换成 DeskPanda：钥匙串 UI 里用户看到的是自己装的那个应用。
#import <Foundation/Foundation.h>
#import <Security/Security.h>
#import <LocalAuthentication/LocalAuthentication.h>
#include <string.h>

int32_t pp_mac_keychain_read(uint8_t *key, size_t capacity) {
    if (!key || capacity != 32) return errSecParam;
    // Legacy login-Keychain ACLs can ignore LAContext noninteraction. Fail closed
    // if the legacy API cannot disable interaction before touching an item.
    OSStatus policy = SecKeychainSetUserInteractionAllowed(false);
    if (policy != errSecSuccess) return policy;
    @autoreleasepool {
        LAContext *context = [LAContext new];
        context.interactionNotAllowed = YES;
        NSDictionary *query = @{
            (__bridge id)kSecClass: (__bridge id)kSecClassGenericPassword,
            (__bridge id)kSecAttrService: @"com.pastepanda.deskpanda.encryption",
            (__bridge id)kSecAttrAccount: @"local-key-v1",
            (__bridge id)kSecReturnData: @YES,
            (__bridge id)kSecMatchLimit: (__bridge id)kSecMatchLimitOne,
            // Explicit policy also covers legacy login-Keychain ACL authorization.
            (__bridge id)kSecUseAuthenticationUI: (__bridge id)kSecUseAuthenticationUIFail,
            (__bridge id)kSecUseAuthenticationContext: context
        };
        CFTypeRef result = NULL;
        OSStatus status = SecItemCopyMatching((__bridge CFDictionaryRef)query, &result);
        if (status != errSecSuccess) return status;
        if (!result || CFGetTypeID(result) != CFDataGetTypeID() || CFDataGetLength((CFDataRef)result) != 32) {
            if (result) CFRelease(result);
            return errSecDecode;
        }
        memcpy(key, CFDataGetBytePtr((CFDataRef)result), 32);
        CFRelease(result);
        return errSecSuccess;
    }
}

int32_t pp_mac_keychain_create(const uint8_t *key, size_t length) {
    if (!key || length != 32) return errSecParam;
    OSStatus policy = SecKeychainSetUserInteractionAllowed(false);
    if (policy != errSecSuccess) return policy;
    @autoreleasepool {
        LAContext *context = [LAContext new];
        context.interactionNotAllowed = YES;
        NSDictionary *item = @{
            (__bridge id)kSecClass: (__bridge id)kSecClassGenericPassword,
            (__bridge id)kSecAttrService: @"com.pastepanda.deskpanda.encryption",
            (__bridge id)kSecAttrAccount: @"local-key-v1",
            (__bridge id)kSecAttrLabel: @"DeskPanda local encryption key",
            (__bridge id)kSecUseAuthenticationUI: (__bridge id)kSecUseAuthenticationUIFail,
            (__bridge id)kSecUseAuthenticationContext: context,
            (__bridge id)kSecValueData: [NSData dataWithBytes:key length:length]
        };
        return SecItemAdd((__bridge CFDictionaryRef)item, NULL);
    }
}
