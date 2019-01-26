using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class StarScript : MonoBehaviour
{
    public Fix64 powerpers = (Fix64)2;
    public GameObject sender;

    private void FixedUpdate()
    {
        perSkill();
    }

    public void perSkill()
    {
        float radius = transform.lossyScale.x / 2;
        Vector2 actionplace = transform.position;
        Collider2D[] colliders = Physics2D.OverlapCircleAll(actionplace, radius);
        foreach (Collider2D hit in colliders)
        {
            HPScript hp = hit.GetComponent<HPScript>();
            if (hp != null)
            {
                if (hp.gameObject == sender)
                    hp.GetHurt(-powerpers * (Fix64)Time.fixedDeltaTime);
                else
                    hp.GetHurt(powerpers * (Fix64)Time.fixedDeltaTime);
            }
        }
    }
}
