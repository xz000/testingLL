using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class SelfExplodeScript : MonoBehaviour
{
    public CooldownImage MyImageScript;
    private float currentcooldown;
    public float cooldowntime = 3;
    public bool skillavaliable;

    // Use this for initialization
    void Start()
    {
        currentcooldown = cooldowntime;
    }

    // Update is called once per frame
    private void FixedUpdate()
    {
        if (skillavaliable)
            return;
        if (currentcooldown >= cooldowntime)
        {
            skillavaliable = true;
        }
        else
        {
            currentcooldown += Time.fixedDeltaTime;
            //MyImageScript.IconFillAmount = currentcooldown / cooldowntime;
        }
    }

    public void Skill()
    {
        //photonView.RPC("RealSkill", PhotonTargets.All);
        //gameObject.GetComponent<MoveScript>().stopwalking();
        //gameObject.GetComponent<StealthScript>().StealthEnd();
        currentcooldown = 0;
        skillavaliable = false;
        float radius = 1.5f;
        Vector2 actionplace = transform.position;
        Fix64 rfix = (Fix64)3 / (Fix64)2;
        Fix64Vector2 apf = new Fix64Vector2(actionplace);
        Collider2D[] colliders = Physics2D.OverlapCircleAll(actionplace, radius + 0.5f);
        foreach (Collider2D hit in colliders)
        {
            HPScript hp = hit.GetComponent<HPScript>();
            if (hp != null)
            {
                if (hit == gameObject.GetComponent<Collider2D>())
                {
                    hp.GetHurt(Mathf.Min(10, hp.currentHP - 1));
                }
                else
                {
                    Rigidbody2D rb = hit.GetComponent<Rigidbody2D>();
                    Fix64Vector2 rbpf = new Fix64Vector2(rb.position);
                    Fix64Vector2 explforce = rbpf - apf;
                    if (explforce.Length() > rfix)
                        continue;
                    hp.GetHurt(10);
                    hit.GetComponent<RBScript>().GetPushed(explforce.normalized() * (Fix64)9, 1f);
                }
            }
        }
    }
}
