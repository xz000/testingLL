using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class ShadowControl : MonoBehaviour
{
    public SpriteRenderer SSR;
    public SpriteRenderer MSR;
    public void ShowSelf()
    {
        SSR.enabled = true;
        MSR.enabled = true;
    }
}
