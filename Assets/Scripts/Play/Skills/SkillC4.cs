using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class SkillC4 : MonoBehaviour
{
    public CooldownImage MyImageScript;
    public GameObject Faker;
    private float currentcooldown;
    public float cooldowntime = 5;
    public bool skillavaliable;
    public float maxfaketime = 3;
    float currentfaketime = 0;
    public Rigidbody2D selfRB;
    bool faking = false;

    // Use this for initialization
    void Start()
    {
        currentcooldown = cooldowntime;
    }

    // Update is called once per frame
    void GoSkillC4()
    {
        if (skillavaliable)
        {
            GetComponent<DoSkill>().singing = 0;
            Skill();
        }
    }

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
        if (faking)
        {
            currentfaketime += Time.fixedDeltaTime;
            if (currentfaketime >= maxfaketime)
            {
                faking = false;
            }
        }
    }

    public void DoFake(Vector2 Worldv2)
    {
        if (!faking)
            return;
        faking = false;
        Fix64Vector2 center = (Fix64Vector2)selfRB.position;
        Fix64Vector2 Realv2 = ((Fix64Vector2)Worldv2 - center).normalized() * (Fix64)2;
        gameObject.GetComponent<DoSkill>().DoClearJob();
        selfRB.position += Realv2.ToV2();
        for (int i = 0; i < 2; i++)
        {
            Realv2 = Realv2.CCWTurn((Fix64)2 * Fix64.Pi / (Fix64)3);
            GameObject nm = Instantiate(Faker, (center + Realv2).ToV2(), Quaternion.identity);
            if (GetComponent<MoveScript>().isme)
                nm.GetComponent<SpriteRenderer>().color = Color.yellow;
            nm.GetComponent<FakeCircleScript>().Beauty = selfRB;
            nm.GetComponent<FakeCircleScript>().maxtime = maxfaketime-currentfaketime;
        }
        GetComponent<MoveScript>().stopwalking();
    }

    void Skill()
    {
        GetComponent<DoSkill>().BeforeSkill();
        currentcooldown = 0;
        skillavaliable = false;
        currentfaketime = 0;
        faking = true;
    }
}
